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

const TITLE_STYLE: tuinix::TerminalStyle = tuinix::TerminalStyle::new().bold();
const INFO_STYLE: tuinix::TerminalStyle = tuinix::TerminalStyle::new().underline();
const BODY_STYLE: tuinix::TerminalStyle = tuinix::TerminalStyle::new();
const MOUSE_STYLE: tuinix::TerminalStyle = tuinix::TerminalStyle::new()
    .bold()
    .fg_color(tuinix::TerminalColor::GREEN);

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

/// Maps a non-blocking I/O result's [`std::io::ErrorKind::WouldBlock`] to `Ok(None)`.
///
/// The input descriptor is non-blocking, so an empty read returns `WouldBlock`
/// rather than blocking the event loop. That becomes `Ok(None)` ("no data right
/// now"), while a genuine error is still passed through unchanged.
fn would_block_as_none<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}

fn draw_header(frame: &mut tuinix::TerminalFrame) {
    write_text(frame, "tuinix Demo\n", TITLE_STYLE);
    write_text(frame, "\nInstructions:\n", INFO_STYLE);
    write_text(
        frame,
        "\u{2022} Click anywhere to see mouse events\n",
        BODY_STYLE,
    );
    write_text(
        frame,
        "\u{2022} Try left, right, and middle mouse buttons\n",
        BODY_STYLE,
    );
    write_text(
        frame,
        "\u{2022} Try scrolling with the mouse wheel\n",
        BODY_STYLE,
    );
    write_text(frame, "\u{2022} Press 'q' to quit\n", BODY_STYLE);
}

fn handle_resize(
    driver: &mut tuinix::TerminalDriver,
    prev_frame: &mut Option<tuinix::TerminalFrame>,
    cursor: Option<tuinix::TerminalPosition>,
) -> std::io::Result<()> {
    let new_size = driver.size()?;
    // Only redraw if the dimensions actually changed. If the previously rendered
    // frame already has the same size, this was a spurious signal; nothing to do.
    if prev_frame.as_ref().is_some_and(|f| f.size() == new_size) {
        return Ok(());
    }
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(new_size);
    draw_header(&mut frame);
    write_text(
        &mut frame,
        &format!(
            "\nLast event: terminal resized to {}x{}\n",
            new_size.cols, new_size.rows
        ),
        INFO_STYLE,
    );
    let out = frame.render(prev_frame.as_ref(), cursor);
    driver.write_all(&out)?;
    driver.flush()?;
    *prev_frame = Some(frame);
    Ok(())
}

/// Reads available input bytes into `input` and draws a reply frame for each parsed
/// event. Returns `false` when the user presses `q` (so the caller should stop the
/// loop); otherwise returns `true` to keep running.
fn handle_input(
    driver: &mut tuinix::TerminalDriver,
    input: &mut tuinix::InputStream,
    prev_frame: &mut Option<tuinix::TerminalFrame>,
    cursor: Option<tuinix::TerminalPosition>,
) -> std::io::Result<bool> {
    let mut raw = [0u8; 256];
    // `n @ 1..` exits the loop on a zero-length read (EOF) without a separate
    // `if n == 0` check: the range pattern only matches when at least one byte
    // was read.
    while let Some(n @ 1..) = would_block_as_none(driver.read(&mut raw))? {
        input.feed(&raw[..n]);
        // Query the physical size for this batch of events so the reply frames are
        // drawn at the terminal's actual dimensions.
        let size = driver.size()?;
        while let Some(event) = input.next() {
            // The header and the render/write/bookkeeping steps are common to every
            // event, so only the event-specific body stays inside the `match`.
            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
            draw_header(&mut frame);

            match event {
                tuinix::TerminalInput::Key(key_input) => {
                    // Quit on 'q'.
                    if let tuinix::KeyCode::Char('q') = key_input.code {
                        return Ok(false);
                    }
                    write_text(
                        &mut frame,
                        &format!("\nLast event: Key pressed: {:?}\n", key_input),
                        INFO_STYLE,
                    );
                }
                tuinix::TerminalInput::Mouse(mouse_input) => {
                    write_text(&mut frame, "\nMouse Event Details:\n", MOUSE_STYLE);
                    write_text(
                        &mut frame,
                        &format!("  Event: {:?}\n", mouse_input.event),
                        BODY_STYLE,
                    );
                    write_text(
                        &mut frame,
                        &format!(
                            "  Position: column {}, row {}\n",
                            mouse_input.position.col, mouse_input.position.row
                        ),
                        BODY_STYLE,
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
                        BODY_STYLE,
                    );
                    write_text(
                        &mut frame,
                        &format!("  Event detail: {:?}\n", mouse_input.event),
                        BODY_STYLE,
                    );
                }
            }

            let out = frame.render(prev_frame.as_ref(), cursor);
            driver.write_all(&out)?;
            driver.flush()?;
            *prev_frame = Some(frame);
        }
    }
    Ok(true)
}

fn main() -> std::io::Result<()> {
    // Initialize the terminal driver and enable mouse input reporting.
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut input = tuinix::InputStream::new();
    let cursor = None;
    let mut prev_frame = None;
    driver.enable_mouse_input()?;

    // Build an initial frame at the terminal's current dimensions.
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(driver.size()?);
    draw_header(&mut frame);
    write_text(&mut frame, "\nLast event: None\n", INFO_STYLE);

    // Render the initial frame and write it to the terminal.
    let out = frame.render(prev_frame.as_ref(), cursor);
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

        if fds[1].revents & libc::POLLIN != 0 {
            handle_resize(&mut driver, &mut prev_frame, cursor)?;
        }

        if fds[0].revents & libc::POLLIN != 0
            && !handle_input(&mut driver, &mut input, &mut prev_frame, cursor)?
        {
            return Ok(());
        }
    }
}
