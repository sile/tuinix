use std::time::Duration;

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
    // Initialize terminal
    let mut terminal = tuinix::Terminal::new()?;

    // Create a frame with the terminal's dimensions
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(terminal.size());

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

    // Draw the frame to the terminal
    terminal.draw(frame)?;

    // Process input events with a timeout
    loop {
        match terminal.poll_event(&[], &[], Some(Duration::from_millis(100)))? {
            Some(tuinix::TerminalEvent::Input(input)) => {
                let tuinix::TerminalInput::Key(input) = input else {
                    continue; // Skip mouse events
                };

                // Check if 'q' was pressed
                if let tuinix::KeyCode::Char('q') = input.code {
                    break;
                }

                // Display the input
                let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(terminal.size());
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
                terminal.draw(frame)?;
            }
            Some(tuinix::TerminalEvent::Resize(size)) => {
                // Terminal was resized, update UI if needed
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
                terminal.draw(frame)?;
            }
            Some(tuinix::TerminalEvent::FdReady { .. }) => unreachable!(),
            None => {
                // Timeout elapsed, no events to process
            }
        }
    }

    Ok(())
}
