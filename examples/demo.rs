//! A complete, end-to-end example of using `tuinix`.
//!
//! This demo drives a [`TerminalDriver`](tuinix::TerminalDriver) from a
//! `libc::poll` event loop, feeding raw bytes into an
//! [`InputDecoder`](tuinix::InputDecoder) and drawing frames with
//! [`Frame::render`](tuinix::Frame::render). It shows:
//!
//! * entering and leaving raw mode (via [`TerminalDriver`](tuinix::TerminalDriver)),
//! * reading raw terminal bytes and turning them into
//!   [`Input`](tuinix::Input) events,
//! * handling keyboard input (quitting on `q`),
//! * reporting a lone Escape key without waiting for the next key,
//! * reporting mouse events,
//! * reacting to a terminal resize.
//!
//! Run it with `cargo run --example demo`.

use std::io::{Read, Write};

const TITLE_STYLE: tuinix::Style = tuinix::Style::new().bold();
const INFO_STYLE: tuinix::Style = tuinix::Style::new().underline();
const BODY_STYLE: tuinix::Style = tuinix::Style::new();
const MOUSE_STYLE: tuinix::Style = tuinix::Style::new().bold().fg_color(tuinix::Color::GREEN);

/// How long to wait for the rest of an escape sequence before a lone `ESC` byte
/// is treated as the Escape key.
///
/// A terminal uses the same byte for the Escape key and for the start of a
/// sequence such as `ESC [ A`, so the two can only be told apart by waiting
/// briefly. 50 ms matches the default of Vim's `ttimeoutlen`.
const ESCAPE_TIMEOUT_MS: libc::c_int = 50;

/// How many bytes of unparsed input to keep before dropping the oldest ones.
///
/// `InputDecoder` does not bound its buffer, so the application decides what to
/// do with input it cannot make sense of. A well-formed sequence is far shorter
/// than this; the bound only stops a never-terminating sequence (for example a
/// truncated mouse report) from growing the buffer without limit.
const MAX_BUFFERED_BYTES: usize = 4096;

/// The tab width [`write_text`] uses.
const TAB_WIDTH: usize = 8;

// NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
// Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
// actual width, because Frame does not compute character widths itself.
//
// The write position is the caller's to keep: it starts where the caller says and
// is threaded through `put_char`, which returns the position just past each
// character. A character that runs past the right edge is dropped rather than
// wrapped, so there is no newline to insert behind the caller's back; a caller
// that wants wrapping wraps. The returned position is where the next write goes.
fn write_text(
    frame: &mut tuinix::Frame,
    at: tuinix::Position,
    text: &str,
    style: tuinix::Style,
) -> tuinix::Position {
    let mut at = at;
    for c in text.chars() {
        match c {
            '\n' => at = at.next_line(),
            '\t' => at = at.next_tab_stop(TAB_WIDTH),
            c if c.is_control() => {}
            c => {
                at = frame.put_char(at, tuinix::Char::new(c, 1, style).expect("valid char"));
            }
        }
    }
    at
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

/// Draws the demo's fixed banner, and returns the position just below it where
/// the caller's own text should start.
fn draw_header(frame: &mut tuinix::Frame) -> tuinix::Position {
    let mut at = tuinix::Position::ORIGIN;
    at = write_text(frame, at, "tuinix Demo\n", TITLE_STYLE);
    at = write_text(frame, at, "\nInstructions:\n", INFO_STYLE);
    at = write_text(
        frame,
        at,
        "\u{2022} Click anywhere to see mouse events\n",
        BODY_STYLE,
    );
    at = write_text(
        frame,
        at,
        "\u{2022} Try left, right, and middle mouse buttons\n",
        BODY_STYLE,
    );
    at = write_text(
        frame,
        at,
        "\u{2022} Try scrolling with the mouse wheel\n",
        BODY_STYLE,
    );
    write_text(frame, at, "\u{2022} Press 'q' to quit\n", BODY_STYLE)
}

fn handle_resize(
    driver: &mut tuinix::TerminalDriver,
    prev_frame: &mut Option<tuinix::Frame>,
    cursor: Option<tuinix::Position>,
) -> std::io::Result<()> {
    driver.handle_resize_signal()?;
    let new_size = driver.size();
    // Only redraw if the dimensions actually changed. If the previously rendered
    // frame already has the same size, this was a spurious signal; nothing to do.
    if prev_frame.as_ref().is_some_and(|f| f.size() == new_size) {
        return Ok(());
    }
    let mut frame = tuinix::Frame::new(new_size);
    let at = draw_header(&mut frame);
    write_text(
        &mut frame,
        at,
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

/// Draws a reply frame for one parsed `event`. Returns `false` when the user
/// presses `q` (so the caller should stop the loop); otherwise returns `true` to
/// keep running.
fn handle_event(
    driver: &mut tuinix::TerminalDriver,
    prev_frame: &mut Option<tuinix::Frame>,
    cursor: Option<tuinix::Position>,
    event: tuinix::Input,
) -> std::io::Result<bool> {
    // The frame is built at the terminal's current dimensions, so a resize is
    // picked up on whichever event is handled first afterwards.
    let mut frame = tuinix::Frame::new(driver.size());
    let at = draw_header(&mut frame);

    match event {
        tuinix::Input::Key(key_input) => {
            // Quit on 'q'.
            if let tuinix::KeyCode::Char('q') = key_input.code {
                return Ok(false);
            }
            write_text(
                &mut frame,
                at,
                &format!("\nLast event: Key pressed: {:?}\n", key_input),
                INFO_STYLE,
            );
        }
        tuinix::Input::Mouse(mouse_input) => {
            let at = write_text(&mut frame, at, "\nMouse Input Details:\n", MOUSE_STYLE);
            let at = write_text(
                &mut frame,
                at,
                &format!("  Kind: {:?}\n", mouse_input.kind),
                BODY_STYLE,
            );
            let at = write_text(
                &mut frame,
                at,
                &format!(
                    "  Position: column {}, row {}\n",
                    mouse_input.position.col, mouse_input.position.row
                ),
                BODY_STYLE,
            );
            write_text(
                &mut frame,
                at,
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
        }
        tuinix::Input::Unrecognized { bytes } => {
            write_text(
                &mut frame,
                at,
                &format!("\nLast event: Undecodable input: {} byte(s)\n", bytes.len()),
                BODY_STYLE,
            );
        }
        tuinix::Input::Paste { bytes } => {
            // A paste is inserted as text rather than interpreted, which is the
            // whole point of reporting it as one input: a newline in the pasted
            // text is a newline, not the Enter key.
            let at = write_text(&mut frame, at, "\nLast event: Paste:\n", INFO_STYLE);
            write_text(
                &mut frame,
                at,
                &format!("  {} byte(s)\n", bytes.len()),
                BODY_STYLE,
            );
        }
    }

    let out = frame.render(prev_frame.as_ref(), cursor);
    driver.write_all(&out)?;
    driver.flush()?;
    *prev_frame = Some(frame);
    Ok(true)
}

/// Reads whatever input bytes are ready into `input`.
///
/// This only moves bytes into `input`; the parsed inputs are drained from it by
/// the caller. Reading and draining are kept apart so that a timeout,
/// which produces no bytes, can still reach the same drain path by committing a
/// lone `ESC` and looping back.
fn read_input(
    driver: &mut tuinix::TerminalDriver,
    input: &mut tuinix::InputDecoder,
) -> std::io::Result<()> {
    let mut raw = [0u8; 256];
    // `n @ 1..` exits the loop on a zero-length read (EOF) without a separate
    // `if n == 0` check: the range pattern only matches when at least one byte
    // was read. Each read is fed immediately so at most one chunk sits in the
    // decoder at a time.
    while let Some(n @ 1..) = would_block_as_none(driver.read(&mut raw))? {
        input.feed(&raw[..n]);
        // The decoder holds bytes only while a sequence is still arriving, so a
        // buffer this large means the source is not speaking terminal input
        // (a mis-sent control sequence, or a stream from a program that is not
        // speaking the protocol). There is nothing to recover: the bytes that
        // would tell us where the next real sequence starts are the ones that
        // never came. Give up rather than decode a stream that no longer means
        // anything.
        if input.buffered_bytes() > MAX_BUFFERED_BYTES {
            eprintln!("input does not look like terminal input; giving up");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    // Initialize the terminal driver and enable mouse input reporting.
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut input = tuinix::InputDecoder::new();
    let cursor = None;
    let mut prev_frame = None;
    driver.enable_mouse_reporting()?;

    // Build an initial frame at the terminal's current dimensions.
    let mut frame = tuinix::Frame::new(driver.size());
    let at = draw_header(&mut frame);
    write_text(&mut frame, at, "\nLast event: None\n", INFO_STYLE);

    // Render the initial frame and write it to the terminal.
    let out = frame.render(prev_frame.as_ref(), cursor);
    driver.write_all(&out)?;
    driver.flush()?;
    prev_frame = Some(frame);

    // Both descriptors are non-blocking, so `poll` waits for readiness instead of
    // blocking on a read.
    let mut fds = [
        libc::pollfd {
            fd: driver.resize_signal_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: driver.input_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];

    loop {
        // A lone `ESC` byte is ambiguous: the terminal reports the Escape key and
        // the start of a sequence such as `ESC [ A` identically. When one is
        // held, wait only briefly so it is reported as Escape promptly instead of
        // sitting there until the next key arrives.
        let timeout = if input.has_uncommitted_escape() {
            ESCAPE_TIMEOUT_MS
        } else {
            -1
        };
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
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

        // A descriptor that hung up or failed can never become ready again, so
        // stop instead of spinning on it.
        if fds
            .iter()
            .any(|fd| fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0)
        {
            return Err(std::io::Error::other("terminal closed"));
        }

        if n == 0 {
            // The wait elapsed with the lone `ESC` still held, so commit it as the
            // Escape key. `revents` stayed empty, so the blocks below are skipped
            // and the drain loop picks the committed Escape up right away.
            input.commit_escape();
        }

        if fds[0].revents & libc::POLLIN != 0 {
            handle_resize(&mut driver, &mut prev_frame, cursor)?;
        }

        if fds[1].revents & libc::POLLIN != 0 {
            read_input(&mut driver, &mut input)?;
        }

        // Drain every event that the bytes fed above (or the committed `ESC`)
        // made available. This is the single place that consumes `InputDecoder`,
        // so a timeout and normal input share one path.
        while let Some(event) = input.next() {
            if !handle_event(&mut driver, &mut prev_frame, cursor, event)? {
                return Ok(());
            }
        }
    }
}
