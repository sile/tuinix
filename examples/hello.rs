use tuinix::terminal::Terminal;

fn main() -> std::io::Result<()> {
    let mut terminal = Terminal::new()?;
    println!("{:?}", terminal.get_size()?);
    std::thread::sleep(std::time::Duration::from_secs(5));

    Ok(())
}
