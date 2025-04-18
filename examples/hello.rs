use tuinix::terminal::Terminal;

fn main() -> std::io::Result<()> {
    let _terminal = Terminal::new()?;
    std::thread::sleep(std::time::Duration::from_secs(5));
    Ok(())
}
