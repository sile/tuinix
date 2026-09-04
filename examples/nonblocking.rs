use std::{fmt::Write, time::Duration};

// Define tokens for our event sources
const STDIN_TOKEN: mio::Token = mio::Token(0);
const SIGNAL_TOKEN: mio::Token = mio::Token(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = tuinix::Terminal::new()?;

    // Set up mio polling
    let mut poll = mio::Poll::new()?;
    let mut events = mio::Events::with_capacity(10);

    // Get the file descriptors we need to monitor.
    // `set_input_nonblocking()` replaces the input fd with a fresh open of the
    // terminal device that stdin is connected to, so making it non-blocking
    // does not affect stdout.
    let stdin_fd = terminal.set_input_nonblocking()?;
    let signal_fd = terminal.set_signal_nonblocking()?;

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
    let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(terminal.size());

    // Add styled content to the frame
    let title_style = tuinix::TerminalStyle::new()
        .bold()
        .fg_color(tuinix::TerminalColor::GREEN);

    writeln!(
        frame,
        "{}Welcome to tuinix!{}",
        title_style,
        tuinix::TerminalStyle::RESET
    )?;
    writeln!(frame, "\nPress any key ('q' to quit)")?;

    // Draw the frame to the terminal
    terminal.draw(frame)?;

    // Event loop
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
                    // Handle keyboard input
                    while let Some(Some(input)) = tuinix::try_nonblocking(terminal.read_input())? {
                        let tuinix::TerminalInput::Key(key_input) = input else {
                            continue; // Skip mouse events
                        };

                        // Check if 'q' was pressed
                        if let tuinix::KeyCode::Char('q') = key_input.code {
                            return Ok(());
                        }

                        // Display the input
                        let mut frame: tuinix::TerminalFrame =
                            tuinix::TerminalFrame::new(terminal.size());
                        writeln!(frame, "Key pressed: {key_input:?}")?;
                        writeln!(frame, "\nPress any key ('q' to quit)")?;
                        terminal.draw(frame)?;
                    }
                }
                SIGNAL_TOKEN => {
                    // Handle terminal resize event
                    while let Some(size) = tuinix::try_nonblocking(terminal.wait_for_resize())? {
                        let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
                        writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
                        writeln!(frame, "\nPress any key ('q' to quit)")?;
                        terminal.draw(frame)?;
                    }
                }
                _ => unreachable!("Unexpected token"),
            }
        }
    }
}
