//! A library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.
//!
//! `tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal
//! dependencies (only `libc` is required). The library offers a clean API for:
//!
//! - Managing the terminal device (raw mode, alternate screen)
//! - Keeping the I/O-free state of a terminal application (size, last frame,
//!   cursor, input buffer)
//! - Capturing and processing keyboard input
//! - Drawing styled text with ANSI colors
//! - Handling terminal resize events
//! - Creating efficient terminal frames with differential updates
//! - Non-blocking input for use with external event loops (`mio` / `tokio`)
//!
//! ## Architecture
//!
//! The library separates the *state* of a terminal application from the *driver*
//! that talks to the terminal device.
//!
//! - [`TerminalState`] is a pure, I/O-free core. It owns the terminal size, the
//!   last frame that was rendered, the cursor position, and the buffer of unparsed
//!   input bytes. It knows how to parse raw bytes into input events and how to
//!   render a frame into a byte buffer, but it never performs I/O itself.
//! - [`TerminalDriver`] owns the file descriptors and terminal modes. It is
//!   responsible for entering and leaving raw mode and the alternate screen, and
//!   it implements [`Read`](std::io::Read) and [`Write`](std::io::Write) so an
//!   application can read raw input bytes from the terminal and write raw output
//!   bytes back to it.
//!
//! The application is responsible for driving the loop: read raw bytes from the
//! driver, feed them into [`TerminalState::feed_bytes()`], pull parsed
//! [`TerminalInput`] values out with [`TerminalState::next_input()`], build a
//! [`TerminalFrame`], ask the state to render it into a byte buffer with
//! [`TerminalState::render()`], and write that buffer to the driver.
//!
//! ## Basic Example
//!
//! This example demonstrates basic terminal UI functionality including initializing the terminal,
//! drawing styled text, processing keyboard events, and handling terminal resizing.
//!
//! ```no_run
//! use std::io::{Read, Write};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize terminal driver and query its size
//!     let mut driver = tuinix::TerminalDriver::new()?;
//!     let mut state = tuinix::TerminalState::new(driver.size()?);
//!
//!     // Create a frame with the terminal's dimensions
//!     let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
//!
//!     // Add styled content to the frame
//!     let title_style = tuinix::TerminalStyle::new().bold().fg_color(tuinix::TerminalColor::GREEN);
//!
//!     // NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
//!     // Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
//!     // actual width, because TerminalFrame does not compute character widths itself.
//!     fn write_text(frame: &mut tuinix::TerminalFrame, text: &str, style: tuinix::TerminalStyle) {
//!         for c in text.chars() {
//!             match c {
//!                 '\n' => frame.push_newline(),
//!                 '\t' => frame.push_tab(8),
//!                 c if c.is_control() => {}
//!                 c => {
//!                     frame.push_char(tuinix::TerminalChar::new(c, 1, style).expect("valid cell"));
//!                 }
//!             }
//!         }
//!     }
//!
//!     write_text(&mut frame, "Welcome to tuinix!", title_style);
//!     write_text(&mut frame, "\nPress any key ('q' to quit)", tuinix::TerminalStyle::new());
//!
//!     // Render the frame to a byte buffer, then write it to the terminal.
//!     let mut out = Vec::new();
//!     state.render(frame, &mut out);
//!     driver.write_all(&out)?;
//!     driver.flush()?;
//!
//!     // Process input events with a timeout
//!     let mut raw = [0u8; 256];
//!     loop {
//!         if state.has_pending_input() && let Some(input) = state.next_input() {
//!             let tuinix::TerminalInput::Key(input) = input else {
//!                 continue;  // Skip mouse events
//!             };
//!
//!             // Check if 'q' was pressed
//!             if let tuinix::KeyCode::Char('q') = input.code {
//!                 break;
//!             }
//!
//!             // Display the input
//!             let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(state.size());
//!             write_text(&mut frame, &format!("Key pressed: {:?}\n", input), tuinix::TerminalStyle::new());
//!             write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::TerminalStyle::new());
//!             let mut out = Vec::new();
//!             state.render(frame, &mut out);
//!             driver.write_all(&out)?;
//!             driver.flush()?;
//!         }
//!
//!         // Read raw bytes from the driver. In a real application this would be
//!         // driven by an event loop (see examples/nonblocking.rs); here we block
//!         // until input arrives.
//!         let n = driver.read(&mut raw)?;
//!         if n == 0 {
//!             continue;
//!         }
//!         state.feed_bytes(&raw[..n]);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! For integration with external event loop libraries like `mio`, see the [nonblocking.rs] example.
//!
//! [nonblocking.rs]: https://github.com/sile/tuinix/blob/main/examples/nonblocking.rs
#![warn(missing_docs)]
use std::{io::ErrorKind, os::fd::RawFd};

mod frame;
mod geometry;
mod input;
mod state;
mod style;
mod terminal;

pub use frame::{TerminalChar, TerminalFrame};
pub use geometry::{TerminalPosition, TerminalRegion, TerminalSize};
pub use input::{KeyCode, KeyInput, MouseEvent, MouseInput, TerminalInput};
pub use state::TerminalState;
pub use style::{TerminalColor, TerminalStyle};
pub use terminal::TerminalDriver;

pub(crate) fn set_fd_nonblocking(fd: RawFd, nonblock: bool) -> std::io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let new_flags = if nonblock {
            flags | libc::O_NONBLOCK
        } else {
            flags & !libc::O_NONBLOCK
        };
        if libc::fcntl(fd, libc::F_SETFL, new_flags) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Handles the result of a non-blocking I/O operation by converting [`ErrorKind::WouldBlock`] errors to `Ok(None)`.
///
/// This utility function is designed to work with non-blocking I/O operations (typically used after
/// calling [`TerminalDriver::set_input_nonblocking()`] and [`TerminalDriver::set_signal_nonblocking()`]). When a non-blocking operation returns a
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
