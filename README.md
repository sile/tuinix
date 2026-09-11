tuinix
======

[![Crates.io](https://img.shields.io/crates/v/tuinix.svg)](https://crates.io/crates/tuinix)
[![Documentation](https://docs.rs/tuinix/badge.svg)](https://docs.rs/tuinix)
[![Actions Status](https://github.com/sile/tuinix/workflows/CI/badge.svg)](https://github.com/sile/tuinix/actions)
![License](https://img.shields.io/crates/l/tuinix)

A Rust library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.

## Overview

`tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal dependencies (only `libc` is required). The library offers a clean API for:

- Managing the terminal device (raw mode, alternate screen)
- Capturing and processing keyboard input
- Drawing styled text with ANSI colors
- Handling terminal resize events
- Creating efficient terminal frames with differential updates
- Non-blocking input and resize notifications for use with external event loops

## Architecture

The library separates the *I/O* of a terminal from the *pure data* that an
application works with.

- [`TerminalDriver`] owns the file descriptors and terminal modes. It is
  responsible for entering and leaving raw mode and the alternate screen, and
  it implements [`Read`](std::io::Read) and [`Write`](std::io::Write) so an
  application can read raw input bytes from the terminal and write raw output
  bytes back to it.
- [`InputDecoder`] is a pure input parser. It accumulates raw bytes and yields
  parsed [`Input`] values, but it never performs I/O itself.
- [`Frame`] is a pure frame buffer. It renders itself into a byte
  buffer, comparing against a previous frame to redraw only what changed, and
  it never performs I/O itself.

The application is responsible for driving the loop: read raw bytes from the
driver, feed them into `InputDecoder::feed()`, pull parsed
`Input` values out with `InputDecoder::next()`, build a
`Frame`, render it into a byte buffer with
`Frame::render()`, and write that buffer to the driver.

## Basic Example

This example demonstrates basic terminal UI functionality including initializing the terminal,
drawing styled text, processing keyboard events, and handling terminal resizing.

```rust,no_run
use std::io::{Read, Write};

// NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
// Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
// actual width, because Frame does not compute character widths itself.
fn write_text(frame: &mut tuinix::Frame, text: &str, style: tuinix::Style) {
    for c in text.chars() {
        match c {
            '\n' => frame.push_newline(),
            '\t' => frame.push_tab(8),
            c if c.is_control() => {}
            c => {
                frame.push_char(tuinix::Char::new(c, 1, style).expect("valid char"));
            }
        }
    }
}

/// Maps a non-blocking I/O result's `WouldBlock` to `Ok(None)`.
fn would_block_as_none<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}

fn main() -> std::io::Result<()> {
    // Initialize the terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut size = driver.size()?;
    let mut input = tuinix::InputDecoder::new();
    let cursor = None;
    let mut prev = None;

    // Add styled content to a frame
    let title_style = tuinix::Style::new().bold().fg_color(tuinix::Color::GREEN);
    let mut frame: tuinix::Frame = tuinix::Frame::new(size);
    write_text(&mut frame, "Welcome to tuinix!\n", title_style);
    write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::Style::new());

    // Render the frame to a byte buffer, then write it to the terminal.
    let out = frame.render(prev.as_ref(), cursor);
    driver.write_all(&out)?;
    driver.flush()?;
    prev = Some(frame);

    // Both descriptors are non-blocking, so `poll` waits for readiness instead
    // of blocking on a read.
    let mut fds = [
        libc::pollfd { fd: driver.signal_fd(), events: libc::POLLIN, revents: 0 },
        libc::pollfd { fd: driver.input_fd(), events: libc::POLLIN, revents: 0 },
    ];
    let mut raw = [0u8; 256];

    loop {
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) } < 0 {
            let err = std::io::Error::last_os_error();
            // `poll` is never restarted by `SA_RESTART`, so a SIGWINCH makes it
            // return `EINTR`. The handler writes the resize byte to the signal
            // pipe before returning, so retrying reports it as `POLLIN`.
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }

        // Handle a terminal resize.
        if fds[0].revents & libc::POLLIN != 0 {
            let new_size = driver.size()?;
            if new_size != size {
                size = new_size;
                let mut frame: tuinix::Frame = tuinix::Frame::new(size);
                write_text(&mut frame, "Welcome to tuinix!\n", title_style);
                write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::Style::new());
                let out = frame.render(prev.as_ref(), cursor);
                driver.write_all(&out)?;
                driver.flush()?;
                prev = Some(frame);
            }
        }

        // Handle available input.
        if fds[1].revents & libc::POLLIN != 0 {
            while let Some(n @ 1..) = would_block_as_none(driver.read(&mut raw))? {
                input.feed(&raw[..n]);
                while let Some(event) = input.next() {
                    let tuinix::Input::Key(key_input) = event else {
                        continue; // Skip mouse events
                    };

                    // Check if 'q' was pressed
                    if let tuinix::KeyCode::Char('q') = key_input.code {
                        return Ok(());
                    }

                    // Display the input
                    let mut frame: tuinix::Frame = tuinix::Frame::new(size);
                    write_text(&mut frame, &format!("Key pressed: {:?}\n", key_input), tuinix::Style::new());
                    write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::Style::new());
                    let out = frame.render(prev.as_ref(), cursor);
                    driver.write_all(&out)?;
                    driver.flush()?;
                    prev = Some(frame);
                }
            }
        }
    }
}
```

A lone `ESC` byte is ambiguous: a terminal sends the same byte whether the user
pressed the Escape key or started a sequence such as `ESC [ A`. Waiting for more
input reports Escape only when the next key arrives, and the two bytes then read
as one Alt+key sequence. The fix is to poll with a short timeout while
`InputDecoder::has_uncommitted_escape()` is `true`, and to commit the byte with
`InputDecoder::commit_escape()` once the wait has elapsed. The committed Escape
key is then returned by `InputDecoder::next()`. Around 50 ms, the default of
Vim's `ttimeoutlen`, is the usual choice.

For a full example of an event loop driven with `poll`, and how to handle keyboard, mouse, and resize events together, see the [demo.rs](examples/demo.rs) example.

The input file descriptor is opened as a fresh, independent description of the
terminal device, so making it non-blocking does not affect the output file
descriptor (which would otherwise share an open file description in typical
interactive terminals, causing `write_all()` to fail with `EAGAIN` /
`EWOULDBLOCK`).
