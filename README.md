tuinix
======

[![Crates.io](https://img.shields.io/crates/v/tuinix.svg)](https://crates.io/crates/tuinix)
[![Documentation](https://docs.rs/tuinix/badge.svg)](https://docs.rs/tuinix)
[![Actions Status](https://github.com/sile/tuinix/workflows/CI/badge.svg)](https://github.com/sile/tuinix/actions
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.

## Overview

`tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal dependencies (only `libc` is required). The library offers a clean API for:

- Managing terminal state (raw mode, alternate screen)
- Capturing and processing keyboard input
- Drawing styled text with ANSI colors
- Handling terminal resize events
- Creating efficient terminal frames with differential updates

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
tuinix = "0.1.0"
```

## Basic Example

```rust
use std::fmt::Write;
use std::time::Duration;
use tuinix::{Terminal, TerminalFrame, TerminalEvent, TerminalInput, TerminalStyle, TerminalColor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Create a frame with the terminal's dimensions
    let mut frame = TerminalFrame::new(terminal.size());

    // Add styled content to the frame
    let title_style = TerminalStyle::new()
        .bold()
        .fg_color(TerminalColor::GREEN);

    writeln!(frame, "{}Welcome to tuinix!{}", title_style, TerminalStyle::RESET)?;
    writeln!(frame, "\nPress 'q' to quit")?;

    // Draw the frame to the terminal
    terminal.draw(frame)?;

    // Process input events with a timeout
    loop {
        match terminal.poll_event(Some(Duration::from_millis(100)))? {
            Some(TerminalEvent::Input(input)) => {
                let TerminalInput::Key(input) = input;

                // Check if 'q' was pressed
                if let tuinix::KeyCode::Char('q') = input.code {
                    break;
                }
            }
            Some(TerminalEvent::Resize(size)) => {
                // Terminal was resized, update UI if needed
                let mut frame = TerminalFrame::new(size);
                writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
                terminal.draw(frame)?;
            }
            None => {
                // Timeout elapsed, no events to process
            }
        }
    }

    Ok(())
}
```

## Non-blocking I/O with External Event Loops

`tuinix` can be integrated with external event loop libraries like `mio`:

```rust
use std::fmt::Write;
use std::time::Duration;
use mio::{Events, Interest, Poll, Token};
use tuinix::{Terminal, TerminalFrame, TerminalInput, set_nonblocking, try_nonblocking};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let mut terminal = Terminal::new()?;

    // Create mio Poll instance
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(10);

    // Get file descriptors and set to non-blocking mode
    let stdin_fd = terminal.input_fd();
    let signal_fd = terminal.signal_fd();
    set_nonblocking(stdin_fd)?;
    set_nonblocking(signal_fd)?;

    // Register with mio poll
    poll.registry().register(
        &mut mio::unix::SourceFd(&stdin_fd),
        Token(0),
        Interest::READABLE
    )?;
    poll.registry().register(
        &mut mio::unix::SourceFd(&signal_fd),
        Token(1),
        Interest::READABLE
    )?;

    // Event loop
    let mut frame = TerminalFrame::new(terminal.size());
    writeln!(frame, "Press 'q' to quit")?;
    terminal.draw(frame)?;

    loop {
        poll.poll(&mut events, Some(Duration::from_millis(100)))?;

        for event in events.iter() {
            match event.token() {
                Token(0) => {
                    // Handle input without blocking
                    if let Some(Some(input)) = try_nonblocking(terminal.read_input())? {
                        let TerminalInput::Key(input) = input;
                        if let tuinix::KeyCode::Char('q') = input.code {
                            return Ok(());
                        }
                    }
                },
                Token(1) => {
                    // Handle terminal resize without blocking
                    if let Some(size) = try_nonblocking(terminal.wait_for_resize())? {
                        let mut frame = TerminalFrame::new(size);
                        writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
                        terminal.draw(frame)?;
                    }
                },
                _ => unreachable!(),
            }
        }
    }
}
```

## Features

- **Lightweight**: Minimal dependencies (only `libc` is required)
- **Terminal Management**: Control terminal state with ease
- **Input Handling**: Process keyboard input events
- **Styled Text**: Supports ANSI colors and text formatting
- **Resize Events**: Gracefully handle terminal resize events
- **Efficient Updates**: Optimize terminal rendering with differential updates
