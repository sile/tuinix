//! A complete, end-to-end example of using `tuinix`.
//!
//! This demo drives a [`TerminalDriver`](tuinix::TerminalDriver) from a
//! `libc::poll` event loop, feeding raw bytes into an
//! [`InputStream`](tuinix::InputStream) and drawing frames with
//! [`TerminalFrame::render`](tuinix::TerminalFrame::render). It shows:
//!
//! * entering and leaving raw mode (via [`TerminalDriver`](tuinix::TerminalDriver)),
//! * reading raw terminal bytes and turning them into
//!   [`TerminalInput`](tuinix::TerminalInput) events,
//! * handling keyboard input (quitting on `q`),
//! * reporting mouse events,
//! * reacting to a terminal resize.
//!
//! Run it with `cargo run --example demo`.

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
            c => {
                frame.push_char(tuinix::TerminalChar::new(c, 1, style).expect("valid cell"));
            }
        }
    }
}

/// Maps a non-blocking I/O call's [`std::io::ErrorKind::WouldBlock`] to `Ok(None)`.
///
/// The input descriptor is non-blocking, so an empty read returns `WouldBlock`
/// rather than blocking the event loop. That becomes `Ok(None)` ("no data right
/// now"), while a genuine error is still passed through unchanged.
fn would_block_as_none<T, F>(call: F) -> std::io::Result<Option<T>>
where
    F: FnOnce() -> std::io::Result<T>,
{
    match call() {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}

fn draw_header(
    frame: &mut tuinix::TerminalFrame,
    title_style: tuinix::TerminalStyle,
    info_style: tuinix::TerminalStyle,
) {
    write_text(frame, "tuinix Demo\n", title_style);
    write_text(frame, "\nInstructions:\n", info_style);
    write_text(
        frame,
        "• Click anywhere to see mouse events\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(
        frame,
        "• Try left, right, and middle mouse buttons\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(
        frame,
        "• Try scrolling with the mouse wheel\n",
        tuinix::TerminalStyle::new(),
    );
    write_text(frame, "• Press 'q' to quit\n", tuinix::TerminalStyle::new());
}

fn main() -> std::io::Result<()> {
    // Initialize the terminal driver and query its size.
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut size = driver.size()?;
    let mut input = tuinix::InputStream::new();
    let cursor = None;
    let mut prev_frame = None;

    // Enable mouse input reporting.
    driver.enable_mouse_input()?;

    // Build an initial frame with the terminal's dimensions.
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
    let title_style = tuinix::TerminalStyle::new().bold();
    let info_style = tuinix::TerminalStyle::new().underline();

    draw_header(&mut frame, title_style, info_style);
    write_text(&mut frame, "\nLast event: None\n", info_style);

    // Render the initial frame and write it to the terminal.
    let mut out = Vec::new();
    frame.render(prev_frame.as_ref(), cursor, &mut out);
    driver.write_all(&out)?;
    driver.flush()?;
    prev_frame = Some(frame);

    // Both descriptors are non-blocking, so `poll` waits for readiness instead of
    // blocking on a read.
    let mut fds = [
        libc::pollfd {
            fd: driver.input_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: driver.signal_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let mut raw = [0u8; 256];

    loop {
        // Wait for a read event on either the input or the signal descriptor.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            // `poll` is never restarted by `SA_RESTART`, so the SIGWINCH handler
            // still makes it return `EINTR`. The handler writes the resize byte to
            // the signal pipe before returning, so retry so `poll` reports it as
            // `POLLIN` on the next iteration.
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }

        // Handle a terminal resize.
        if fds[1].revents & libc::POLLIN != 0 {
            let new_size = driver.size()?;
            if new_size != size {
                size = new_size;
                let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                draw_header(&mut frame, title_style, info_style);
                write_text(
                    &mut frame,
                    &format!(
                        "\nLast event: terminal resized to {}x{}\n",
                        size.cols, size.rows
                    ),
                    info_style,
                );
                let mut out = Vec::new();
                frame.render(prev_frame.as_ref(), cursor, &mut out);
                driver.write_all(&out)?;
                driver.flush()?;
                prev_frame = Some(frame);
            }
        }

        // Handle available input.
        if fds[0].revents & libc::POLLIN != 0 {
            while let Some(n) = would_block_as_none(|| driver.read(&mut raw))? {
                if n == 0 {
                    break;
                }
                input.feed(&raw[..n]);
                while let Some(event) = input.next() {
                    match event {
                        tuinix::TerminalInput::Key(key_input) => {
                            // Quit on 'q'.
                            if let tuinix::KeyCode::Char('q') = key_input.code {
                                return Ok(());
                            }

                            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                            draw_header(&mut frame, title_style, info_style);
                            write_text(
                                &mut frame,
                                &format!("\nLast event: Key pressed: {:?}\n", key_input),
                                info_style,
                            );
                            let mut out = Vec::new();
                            frame.render(prev_frame.as_ref(), cursor, &mut out);
                            driver.write_all(&out)?;
                            driver.flush()?;
                            prev_frame = Some(frame);
                        }
                        tuinix::TerminalInput::Mouse(mouse_input) => {
                            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                            draw_header(&mut frame, title_style, info_style);

                            let event_style = tuinix::TerminalStyle::new()
                                .bold()
                                .fg_color(tuinix::TerminalColor::GREEN);
                            write_text(&mut frame, "\nMouse Event Details:\n", event_style);
                            write_text(
                                &mut frame,
                                &format!("  Event: {:?}\n", mouse_input.event),
                                tuinix::TerminalStyle::new(),
                            );
                            write_text(
                                &mut frame,
                                &format!(
                                    "  Position: column {}, row {}\n",
                                    mouse_input.position.col, mouse_input.position.row
                                ),
                                tuinix::TerminalStyle::new(),
                            );
                            write_text(
                                &mut frame,
                                &format!(
                                    "  Modifiers: {}\n",
                                    [
                                        mouse_input.ctrl.then_some("Ctrl"),
                                        mouse_input.alt.then_some("Alt"),
                                        mouse_input.shift.then_some("Shift"),
                                    ]
                                    .into_iter()
                                    .flatten()
                                    .collect::<Vec<_>>()
                                    .join(" + ")
                                ),
                                tuinix::TerminalStyle::new(),
                            );

                            write_text(
                                &mut frame,
                                &format!("  Event detail: {:?}\n", mouse_input.event),
                                tuinix::TerminalStyle::new(),
                            );

                            let mut out = Vec::new();
                            frame.render(prev_frame.as_ref(), cursor, &mut out);
                            driver.write_all(&out)?;
                            driver.flush()?;
                            prev_frame = Some(frame);
                        }
                    }
                }
            }
        }
    }
}
