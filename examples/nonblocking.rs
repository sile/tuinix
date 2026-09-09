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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal driver and query its size
    let mut driver = tuinix::TerminalDriver::new()?;
    let mut size = driver.size()?;
    let mut input = tuinix::InputStream::new();
    let cursor = None;
    let mut prev = None;

    // The input and signal descriptors are both non-blocking. They can be
    // monitored with `poll` so that an event loop can react to either keyboard
    // input or a terminal resize as they arrive.
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
        // Wait for a read event on either the input or the signal descriptor.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if n < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        // Handle a terminal resize.
        if fds[1].revents & libc::POLLIN != 0 {
            while let Some(new_size) = tuinix::try_nonblocking(driver.poll_resize())? {
                size = new_size;
                let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                write_text(
                    &mut frame,
                    &format!("Terminal resized to {}x{}\n", size.cols, size.rows),
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

        // Handle available input.
        if fds[0].revents & libc::POLLIN != 0 {
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
    }
}
