tuinix
======

[![Crates.io](https://img.shields.io/crates/v/tuinix.svg)](https://crates.io/crates/tuinix)
[![Documentation](https://docs.rs/tuinix/badge.svg)](https://docs.rs/tuinix)
[![Actions Status](https://github.com/sile/tuinix/workflows/CI/badge.svg)](https://github.com/sile/tuinix/actions)
![License](https://img.shields.io/crates/l/tuinix)

A Rust library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.

## Overview

`tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal dependencies (only `libc` is required). The library offers a clean API for:

- Managing terminal state (raw mode, alternate screen)
- Capturing and processing keyboard input
- Drawing styled text with ANSI colors
- Handling terminal resize events
- Creating efficient terminal frames with differential updates

## Basic Example

This example demonstrates basic terminal UI functionality including initializing the terminal,
drawing styled text, processing keyboard events, and handling terminal resizing.

```rust
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
                writeln!(frame, "Key pressed: {:?}", input)?;
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
```

For integration with external event loop libraries like `mio`, see the [nonblocking.rs](examples/nonblocking.rs) example.
