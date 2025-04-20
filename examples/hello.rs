use tuinix::{frame::TerminalPosition, terminal::Terminal};

fn main() -> std::io::Result<()> {
    let mut terminal = Terminal::new()?;
    terminal.set_cursor(Some(TerminalPosition::row_col(2, 2)))?;
    println!("{:?}", terminal.size());
    terminal.set_cursor(None)?;

    for _ in 0..5 {
        let event = terminal.poll_event(Some(std::time::Duration::from_millis(1000)))?;
        if let Some(event) = event {
            dbg!(event);
        }
    }

    Ok(())
}
