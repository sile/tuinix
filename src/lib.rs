//! A library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.
//!
//! `tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal
//! dependencies (only `libc` is required). The library offers a clean API for:
//!
//! - Managing terminal state (raw mode, alternate screen)
//! - Capturing and processing keyboard input
//! - Drawing styled text with ANSI colors
//! - Handling terminal resize events
//! - Creating efficient terminal frames with differential updates
//!
//! ## Basic Example
//!
//! ```no_run
//! use std::fmt::Write;
//! use std::time::Duration;
//! use tuinix::{Terminal, TerminalFrame, TerminalEvent, TerminalInput, TerminalStyle, TerminalColor};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize terminal
//!     let mut terminal = Terminal::new()?;
//!
//!     // Create a frame with the terminal's dimensions
//!     let mut frame = TerminalFrame::new(terminal.size());
//!
//!     // Add styled content to the frame
//!     let title_style = TerminalStyle::new()
//!         .bold()
//!         .fg_color(TerminalColor::GREEN);
//!
//!     writeln!(frame, "{}Welcome to tuinix!{}", title_style, TerminalStyle::RESET)?;
//!     writeln!(frame, "\nPress 'q' to quit")?;
//!
//!     // Draw the frame to the terminal
//!     terminal.draw(frame)?;
//!
//!     // Process input events with a timeout
//!     loop {
//!         match terminal.poll_event(Some(Duration::from_millis(100)))? {
//!             Some(TerminalEvent::Input(input)) => {
//!                 let TerminalInput::Key(input) = input;
//!
//!                 // Check if 'q' was pressed
//!                 if let tuinix::KeyCode::Char('q') = input.code {
//!                     break;
//!                 }
//!             }
//!             Some(TerminalEvent::Resize(size)) => {
//!                 // Terminal was resized, update UI if needed
//!                 let mut frame = TerminalFrame::new(size);
//!                 writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
//!                 terminal.draw(frame)?;
//!             }
//!             None => {
//!                 // Timeout elapsed, no events to process
//!             }
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Non-blocking I/O with External Event Loops
//!
//! `tuinix` can be integrated with external event loop libraries like `mio`:
//!
//! ```no_run
//! use std::fmt::Write;
//! use std::time::Duration;
//! use mio::{Events, Interest, Poll, Token};
//! use tuinix::{Terminal, TerminalFrame, TerminalInput, set_nonblocking, try_nonblocking};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize terminal
//!     let mut terminal = Terminal::new()?;
//!
//!     // Create mio Poll instance
//!     let mut poll = Poll::new()?;
//!     let mut events = Events::with_capacity(10);
//!
//!     // Get file descriptors and set to non-blocking mode
//!     let stdin_fd = terminal.input_fd();
//!     let signal_fd = terminal.signal_fd();
//!     set_nonblocking(stdin_fd)?;
//!     set_nonblocking(signal_fd)?;
//!
//!     // Register with mio poll
//!     poll.registry().register(
//!         &mut mio::unix::SourceFd(&stdin_fd),
//!         Token(0),
//!         Interest::READABLE
//!     )?;
//!     poll.registry().register(
//!         &mut mio::unix::SourceFd(&signal_fd),
//!         Token(1),
//!         Interest::READABLE
//!     )?;
//!
//!     // Event loop
//!     let mut frame = TerminalFrame::new(terminal.size());
//!     writeln!(frame, "Press 'q' to quit")?;
//!     terminal.draw(frame)?;
//!
//!     loop {
//!         poll.poll(&mut events, Some(Duration::from_millis(100)))?;
//!
//!         for event in events.iter() {
//!             match event.token() {
//!                 Token(0) => {
//!                     // Handle input without blocking
//!                     if let Some(Some(input)) = try_nonblocking(terminal.read_input())? {
//!                         let TerminalInput::Key(input) = input;
//!                         if let tuinix::KeyCode::Char('q') = input.code {
//!                             return Ok(());
//!                         }
//!                     }
//!                 },
//!                 Token(1) => {
//!                     // Handle terminal resize without blocking
//!                     if let Some(size) = try_nonblocking(terminal.wait_for_resize())? {
//!                         let mut frame = TerminalFrame::new(size);
//!                         writeln!(frame, "Terminal resized to {}x{}", size.cols, size.rows)?;
//!                         terminal.draw(frame)?;
//!                     }
//!                 },
//!                 _ => unreachable!(),
//!             }
//!         }
//!     }
//! }
//! ```
#![warn(missing_docs)]
use std::{io::ErrorKind, os::fd::RawFd};

mod frame;
mod geometry;
mod input;
mod style;
mod terminal;

pub use frame::{FixedCharWidthMeasurer, MeasureCharWidth, TerminalFrame};
pub use geometry::{TerminalPosition, TerminalSize};
pub use input::{KeyCode, KeyInput, TerminalInput};
pub use style::{TerminalColor, TerminalStyle};
pub use terminal::{Terminal, TerminalEvent};

/// Sets a file descriptor to non-blocking mode.
///
/// This function modifies the flags of the given file descriptor (`fd`) to
/// include the `O_NONBLOCK` flag, which makes operations on the file descriptor
/// non-blocking.
///
/// When a file descriptor is in non-blocking mode, operations that would normally
/// block until data is available (such as `read`) or until resources are ready
/// (such as `write`) will instead immediately return with [`std::io::ErrorKind::WouldBlock`]
/// if the operation cannot be completed without blocking. This allows the calling
/// thread to continue execution and check for availability later, which is
/// particularly useful in asynchronous I/O patterns.
pub fn set_nonblocking(fd: RawFd) -> std::io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Handles the result of a non-blocking I/O operation by converting [`ErrorKind::WouldBlock`] errors to `Ok(None)`.
///
/// This utility function is designed to work with non-blocking I/O operations (typically used after
/// calling [`set_nonblocking()`] on [`Terminal::input_fd()`] and [`Terminal::signal_fd()`]). When a non-blocking operation returns a
/// [`ErrorKind::WouldBlock`] error, indicating that the operation would need to block to complete, this function
/// converts it to `Ok(None)` for easier handling in caller code.
pub fn try_nonblocking<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
    match result {
        Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
        Ok(v) => Ok(Some(v)),
    }
}

/// Handles the result of an I/O operation that might be interrupted by converting [`ErrorKind::Interrupted`] errors to `Ok(None)`.
///
/// This utility function manages system calls that can be interrupted by signals. When an I/O operation
/// returns an [`ErrorKind::Interrupted`] error, indicating that a system call was interrupted by a signal
/// before it could complete, this function converts it to `Ok(None)` for easier handling in caller code.
///
/// This is particularly useful in scenarios where you want to retry operations that were interrupted,
/// rather than propagating the error.
pub fn try_uninterrupted<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
    match result {
        Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e),
        Ok(v) => Ok(Some(v)),
    }
}
