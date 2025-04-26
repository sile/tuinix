use mio::{Events, Interest, Poll, Token};
use std::fmt::Write;
use std::time::Duration;
use tuinix::{
    KeyCode, KeyInput, Terminal, TerminalFrame, TerminalInput, set_nonblocking, try_nonblocking,
    try_uninterrupted,
};

const STDIN: Token = Token(0);
const SIGNAL: Token = Token(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the terminal
    let mut terminal = Terminal::new()?;

    // Set up initial display
    let mut frame = TerminalFrame::new(terminal.size());
    writeln!(frame)?;
    writeln!(frame, "  Non-blocking Terminal Example (press 'q' to quit)")?;

    writeln!(frame)?;
    writeln!(frame, "  Terminal size: {:?}", terminal.size())?;

    writeln!(frame)?;
    writeln!(frame, "  Waiting for input events...")?;

    terminal.draw(frame)?;

    // Set up mio poll
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(10);

    // Get file descriptors from terminal
    let stdin_fd = terminal.input_fd();
    let signal_fd = terminal.signal_fd();

    // Set fds to non-blocking mode
    set_nonblocking(stdin_fd)?;
    set_nonblocking(signal_fd)?;

    // Register sources with mio
    let mut stdin_source = mio::unix::SourceFd(&stdin_fd);
    let mut signal_source = mio::unix::SourceFd(&signal_fd);

    poll.registry()
        .register(&mut stdin_source, STDIN, Interest::READABLE)?;

    poll.registry()
        .register(&mut signal_source, SIGNAL, Interest::READABLE)?;

    // Main event loop
    'main: loop {
        // Wait for events with a timeout
        if try_uninterrupted(poll.poll(&mut events, Some(Duration::from_millis(1000))))?.is_none() {
            continue;
        }

        // Process events
        for event in events.iter() {
            match event.token() {
                STDIN => {
                    // Handle input events
                    while let Some(input) = try_nonblocking(terminal.read_input())? {
                        let Some(input) = input else {
                            continue;
                        };

                        let mut frame = TerminalFrame::new(terminal.size());

                        writeln!(frame)?;
                        writeln!(frame, "  Non-blocking Terminal Example (press 'q' to quit)")?;

                        writeln!(frame)?;
                        writeln!(frame, "  Terminal size: {:?}", terminal.size())?;

                        writeln!(frame)?;
                        writeln!(frame, "  Received input: {:?}", input)?;
                        writeln!(frame, "  Received input: {:?}", input)?;

                        // Quit when 'q' is pressed
                        if let TerminalInput::Key(KeyInput {
                            code: KeyCode::Char('q'),
                            ..
                        }) = input
                        {
                            break 'main;
                        }

                        terminal.draw(frame)?;
                    }
                }
                SIGNAL => {
                    // Handle terminal resize events
                    while let Some(size) = try_nonblocking(terminal.wait_for_resize())? {
                        let mut frame = TerminalFrame::new(size);

                        writeln!(frame)?;
                        writeln!(frame, "  Non-blocking Terminal Example (press 'q' to quit)")?;

                        writeln!(frame)?;
                        writeln!(frame, "  Terminal size: {:?} (resized)", size)?;

                        writeln!(frame)?;
                        writeln!(frame, "  Waiting for input events...")?;

                        terminal.draw(frame)?;
                    }
                }
                _ => unreachable!("Unexpected token"),
            }
        }

        // If no events occurred, this could be a timeout
        if events.is_empty() {
            let mut frame = TerminalFrame::new(terminal.size());

            writeln!(frame)?;
            writeln!(frame, "  Non-blocking Terminal Example (press 'q' to quit)")?;

            writeln!(frame)?;
            writeln!(frame, "  Terminal size: {:?}", terminal.size())?;

            writeln!(frame)?;
            writeln!(frame, "  Tick... (waiting for events)")?;

            terminal.draw(frame)?;
        }
    }

    Ok(())
}
