use std::io::{Read, Write};
use std::time::Duration;

// Define tokens for our event sources
const STDIN_TOKEN: mio::Token = mio::Token(0);
const SIGNAL_TOKEN: mio::Token = mio::Token(1);

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let size = driver.size()?;
    let mut input = tuinix::InputStream::new();
    let cursor = None;
    let mut prev = None;

    // Set up mio polling
    let mut poll = mio::Poll::new()?;
    let mut events = mio::Events::with_capacity(10);

    // Get the file descriptors we need to monitor.
    // `set_input_nonblocking()` replaces the input fd with a fresh open of the
    // terminal device that stdin is connected to, so making it non-blocking
    // does not affect stdout.
    let stdin_fd = driver.set_input_nonblocking()?;
    let signal_fd = driver.set_signal_nonblocking()?;

    // Register the file descriptors with mio
    poll.registry().register(
        &mut mio::unix::SourceFd(&stdin_fd),
        STDIN_TOKEN,
        mio::Interest::READABLE,
    )?;
    poll.registry().register(
        &mut mio::unix::SourceFd(&signal_fd),
        SIGNAL_TOKEN,
        mio::Interest::READABLE,
    )?;

    // Draw initial frame
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new()
        .bold()
        .fg_color(tuinix::TerminalColor::GREEN);

    write_text(&mut frame, "Welcome to tuinix!\n", title_style);
    write_text(
        &mut frame,
        "\nPress any key ('q' to quit)\n",
        tuinix::TerminalStyle::new(),
    );

    // Render the frame to a byte buffer, then write it to the terminal.
    let mut out = Vec::new();
    frame.render(prev.as_ref(), cursor, &mut out);
    driver.write_all(&out)?;
    driver.flush()?;
    prev = Some(frame);

    // Event loop
    let mut raw = [0u8; 256];
    loop {
        // Wait for events with a timeout
        if tuinix::try_uninterrupted(poll.poll(&mut events, Some(Duration::from_millis(100))))?
            .is_none()
        {
            continue;
        }

        for event in events.iter() {
            match event.token() {
                STDIN_TOKEN => {
                    // Handle keyboard input by reading raw bytes and feeding them
                    // into the input stream.
                    while let Some(n) = tuinix::try_nonblocking(driver.read(&mut raw))? {
                        if n == 0 {
                            break;
                        }
                        input.feed(&raw[..n]);
                        while let Some(event) = input.next() {
                            let tuinix::TerminalInput::Key(key_input) = event else {
                                continue; // Skip mouse events
                            };

                            // Check if 'q' was pressed
                            if let tuinix::KeyCode::Char('q') = key_input.code {
                                return Ok(());
                            }

                            // Display the input
                            let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                            write_text(
                                &mut frame,
                                &format!("Key pressed: {:?}\n", key_input),
                                tuinix::TerminalStyle::new(),
                            );
                            write_text(
                                &mut frame,
                                "\nPress any key ('q' to quit)\n",
                                tuinix::TerminalStyle::new(),
                            );
                            let mut out = Vec::new();
                            frame.render(prev.as_ref(), cursor, &mut out);
                            driver.write_all(&out)?;
                            driver.flush()?;
                            prev = Some(frame);
                        }
                    }
                }
                SIGNAL_TOKEN => {
                    // Handle terminal resize event
                    while let Some(new_size) = tuinix::try_nonblocking(driver.poll_resize())? {
                        let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(new_size);
                        write_text(
                            &mut frame,
                            &format!("Terminal resized to {}x{}\n", new_size.cols, new_size.rows),
                            tuinix::TerminalStyle::new(),
                        );
                        write_text(
                            &mut frame,
                            "\nPress any key ('q' to quit)\n",
                            tuinix::TerminalStyle::new(),
                        );
                        let mut out = Vec::new();
                        frame.render(prev.as_ref(), cursor, &mut out);
                        driver.write_all(&out)?;
                        driver.flush()?;
                        prev = Some(frame);
                    }
                }
                _ => unreachable!("Unexpected token"),
            }
        }
    }
}
