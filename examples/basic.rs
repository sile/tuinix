use std::{fmt::Write, time::Duration};

use tuinix::{Terminal, TerminalColor, TerminalEvent, TerminalFrame, TerminalInput, TerminalStyle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Create a frame with the terminal's dimensions
    let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());

    // Add styled content to the frame
    let title_style = TerminalStyle::new().bold().fg_color(TerminalColor::GREEN);

    writeln!(
        frame,
        "{}Welcome to tuinix!{}",
        title_style,
        TerminalStyle::RESET
    )?;
    writeln!(frame, "\nPress any key ('q' to quit)")?;

    // Draw the frame to the terminal
    terminal.draw(frame)?;

    // Process input events with a timeout
    loop {
        match terminal.poll_event(&[], &[], Some(Duration::from_millis(100)))? {
            Some(TerminalEvent::Input(input)) => {
                let TerminalInput::Key(input) = input;

                // Check if 'q' was pressed
                if let tuinix::KeyCode::Char('q') = input.code {
                    break;
                }

                // Display the input
                let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());
                writeln!(frame, "Key pressed: {input:?}")?;
                writeln!(frame, "\nPress any key ('q' to quit)")?;
                terminal.draw(frame)?;
            }
            Some(TerminalEvent::Resize(size)) => {
                // Terminal was resized, update UI if needed
                let mut frame: TerminalFrame = TerminalFrame::new(size);
                writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
                writeln!(frame, "\nPress any key ('q' to quit)")?;
                terminal.draw(frame)?;
            }
            Some(TerminalEvent::FdReady { .. }) => unreachable!(),
            None => {
                // Timeout elapsed, no events to process
            }
        }
    }

    Ok(())
}
