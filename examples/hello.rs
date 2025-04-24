use std::fmt::Write;

use tuinix::{Terminal, TerminalPosition, frame::TerminalFrame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new()?;

    let mut frame = TerminalFrame::new(terminal.size());
    frame.set_cursor(TerminalPosition::row_col(2, 2));
    write!(frame, "Hello World: {:?}", terminal.size())?;
    terminal.draw(frame)?;

    for _ in 0..5 {
        let event = terminal.poll_event(Some(std::time::Duration::from_millis(1000)))?;
        if let Some(event) = event {
            let mut frame = TerminalFrame::new(terminal.size());
            frame.set_cursor(TerminalPosition::row_col(2, 2));
            write!(frame, "Hello World: {:?}", event)?;
            terminal.draw(frame)?;
        }
    }

    Ok(())
}
