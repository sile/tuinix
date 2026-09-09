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
- Keeping the I/O-free state of a terminal application (size, last frame,
  cursor, input buffer)
- Capturing and processing keyboard input
- Drawing styled text with ANSI colors
- Handling terminal resize events
- Creating efficient terminal frames with differential updates
- Non-blocking input for use with external event loops (`mio` / `tokio`)

## Architecture

The library separates the *state* of a terminal application from the
*driver* that talks to the terminal device.

- [`TerminalState`] is a pure, I/O-free core. It owns the terminal size, the
  last frame that was rendered, the cursor position, and the buffer of unparsed
  input bytes. It knows how to parse raw bytes into input events and how to
  render a frame into a byte buffer, but it never performs I/O itself.
- [`TerminalDriver`] owns the file descriptors and terminal modes. It is
  responsible for entering and leaving raw mode and the alternate screen, and
  it implements [`Read`](std::io::Read) and [`Write`](std::io::Write) so an
  application can read raw input bytes from the terminal and write raw output
  bytes back to it.

The application is responsible for driving the loop: read raw bytes from the
driver, feed them into `TerminalState::push_input()`, pull parsed
`TerminalInput` values out with `TerminalState::next_input()`, build a
`TerminalFrame`, ask the state to render it into a byte buffer with
`TerminalState::render()`, and write that buffer to the driver.

## Basic Example

This example demonstrates basic terminal UI functionality including initializing the terminal,
drawing styled text, processing keyboard events, and handling terminal resizing.

```rust
use std::io::{Read, Write};

// NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
// Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
// actual width, because TerminalFrame does not compute character widths itself.
fn write_text(frame: &mut tuinix::TerminalFrame, text: &str, style: tuinix::TerminalStyle) {
    for c in text.chars() {
        match c {
            '\n' => frame.push_newline(),
            '\t' => frame.push_tab(8),
            c if c.is_control() => {}
            c => frame.push_char(tuinix::TerminalChar::new(c, 1, style).expect("valid cell")),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut state = tuinix::TerminalState::new(driver.size()?);

    // Create a frame with the terminal's dimensions
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new().bold().fg_color(tuinix::TerminalColor::GREEN);

    write_text(&mut frame, "Welcome to tuinix!\n", title_style);
    write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::TerminalStyle::new());

    // Render the frame to a byte buffer, then write it to the terminal.
    let mut out = Vec::new();
    state.render(frame, &mut out);
    driver.write_all(&out)?;
    driver.flush()?;

    // Process input events with a timeout
    let mut raw = [0u8; 256];
    loop {
        if state.has_pending_input() && let Some(input) = state.next_input() {
            let tuinix::TerminalInput::Key(input) = input else {
                continue; // Skip mouse events
            };

            // Check if 'q' was pressed
            if let tuinix::KeyCode::Char('q') = input.code {
                break;
            }

            // Display the input
            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
            write_text(
                &mut frame,
                &format!("Key pressed: {:?}\n", input),
                tuinix::TerminalStyle::new(),
            );
            write_text(
                &mut frame,
                "\nPress any key ('q' to quit)\n",
                tuinix::TerminalStyle::new(),
            );
            let mut out = Vec::new();
            state.render(frame, &mut out);
            driver.write_all(&out)?;
            driver.flush()?;
        }

        // Read raw bytes from the driver. In a real application this would be
        // driven by an event loop (see examples/nonblocking.rs); here we block
        // until input arrives.
        let n = driver.read(&mut raw)?;
        if n == 0 {
            continue;
        }
        state.push_input(&raw[..n]);
    }

    Ok(())
}
```

For integration with external event loop libraries like `mio`, see the [nonblocking.rs](examples/nonblocking.rs) example.

Note that making the terminal input fd non-blocking directly (e.g. via `fcntl` with
`O_NONBLOCK`) also affects the output fd, because both share an open file description in typical
interactive terminals. This can make `write_all()` fail with `EAGAIN` / `EWOULDBLOCK`. Use
`TerminalDriver::set_input_nonblocking()` instead to make the input non-blocking.
