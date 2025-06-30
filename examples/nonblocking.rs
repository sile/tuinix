use mio::{Events, Interest, Poll, Token};
use std::{fmt::Write, time::Duration};
use tuinix::{
    KeyCode, Terminal, TerminalColor, TerminalFrame, TerminalInput, TerminalStyle, set_nonblocking,
    try_nonblocking, try_uninterrupted,
};

// Define tokens for our event sources
const STDIN_TOKEN: Token = Token(0);
const SIGNAL_TOKEN: Token = Token(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Set up mio polling
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(10);

    // Get the file descriptors we need to monitor
    let stdin_fd = terminal.input_fd();
    let signal_fd = terminal.signal_fd();

    // Set both file descriptors to non-blocking mode
    set_nonblocking(stdin_fd)?;
    set_nonblocking(signal_fd)?;

    // Register the file descriptors with mio
    poll.registry().register(
        &mut mio::unix::SourceFd(&stdin_fd),
        STDIN_TOKEN,
        Interest::READABLE,
    )?;
    poll.registry().register(
        &mut mio::unix::SourceFd(&signal_fd),
        SIGNAL_TOKEN,
        Interest::READABLE,
    )?;

    // Draw initial frame
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

    // Event loop
    loop {
        // Wait for events with a timeout
        if try_uninterrupted(poll.poll(&mut events, Some(Duration::from_millis(100))))?.is_none() {
            continue;
        }

        for event in events.iter() {
            match event.token() {
                STDIN_TOKEN => {
                    // Handle keyboard input
                    while let Some(Some(input)) = try_nonblocking(terminal.read_input())? {
                        match input {
                            TerminalInput::Key(key_input) => {
                                // Check if 'q' was pressed
                                if let KeyCode::Char('q') = key_input.code {
                                    return Ok(());
                                }

                                // Display the input
                                let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());
                                writeln!(frame, "Key pressed: {key_input:?}")?;
                                writeln!(frame, "\nPress any key ('q' to quit)")?;
                                terminal.draw(frame)?;
                            }
                        }
                    }
                }
                SIGNAL_TOKEN => {
                    // Handle terminal resize event
                    while let Some(size) = try_nonblocking(terminal.wait_for_resize())? {
                        let mut frame: TerminalFrame = TerminalFrame::new(size);
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
