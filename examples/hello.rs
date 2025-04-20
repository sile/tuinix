use tuinix::{frame::TerminalPosition, terminal::Terminal};

fn main() -> std::io::Result<()> {
    let mut terminal = Terminal::new()?;
    terminal.set_cursor(Some(TerminalPosition::row_col(2, 2)))?;
    println!("{:?}", terminal.size());
    terminal.set_cursor(None)?;
    std::thread::sleep(std::time::Duration::from_secs(5));

    Ok(())
}
