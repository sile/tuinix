//! A library for building terminal user interface (TUI) applications on Unix systems with minimum dependencies.
//!
//! `tuinix` provides a lightweight foundation for building terminal-based user interfaces with minimal
//! dependencies (only `libc` is required). The library offers a clean API for:
//!
//! - Managing the terminal device (raw mode, alternate screen)
//! - Capturing and processing keyboard input
//! - Drawing styled text with ANSI colors
//! - Handling terminal resize events
//! - Creating efficient terminal frames with differential updates
//! - Non-blocking input and resize notifications for use with external event loops
//!
//! ## Architecture
//!
//! The library separates the *I/O* of a terminal from the *pure data* that an
//! application works with.
//!
//! - [`InputStream`] is a pure input parser. It accumulates raw bytes and yields
//!   parsed [`TerminalInput`] values, but it never performs I/O itself.
//! - [`TerminalFrame`] is a pure frame buffer. It renders itself into a byte
//!   buffer, comparing against a previous frame to redraw only what changed, and
//!   it never performs I/O itself.
//! - [`TerminalDriver`] owns the file descriptors and terminal modes. It is
//!   responsible for entering and leaving raw mode and the alternate screen, and
//!   it implements [`Read`](std::io::Read) and [`Write`](std::io::Write) so an
//!   application can read raw input bytes from the terminal and write raw output
//!   bytes back to it.
//!
//! The application is responsible for driving the loop: read raw bytes from the
//! driver, feed them into [`InputStream::feed()`], pull parsed
//! [`TerminalInput`] values out with [`InputStream::next()`], build a
//! [`TerminalFrame`], render it into a byte buffer with
//! [`TerminalFrame::render()`], and write that buffer to the driver.
//!
//! ## Basic Example
//!
//! This example demonstrates basic terminal UI functionality including initializing the terminal,
//! drawing styled text, processing keyboard events, and handling terminal resizing.
//!
//! ```no_run
//! use std::io::{Read, Write};
//!
//! fn main() -> std::io::Result<()> {
//!     // Initialize terminal driver and query its size
//!     let mut driver = tuinix::TerminalDriver::new()?;
//!     let mut size = driver.size()?;
//!     let mut input = tuinix::InputStream::new();
//!     let cursor = None;
//!     let mut prev = None;
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
//!                     frame.push_char(tuinix::TerminalChar::new(c, 1, style).expect("valid char"));
//!                 }
//!             }
//!         }
//!     }
//!
//!     // Add styled content to a frame
//!     let title_style = tuinix::TerminalStyle::new().bold().fg_color(tuinix::TerminalColor::GREEN);
//!     let mut frame = tuinix::TerminalFrame::new(size);
//!     write_text(&mut frame, "Welcome to tuinix!", title_style);
//!     write_text(&mut frame, "\nPress any key ('q' to quit)", tuinix::TerminalStyle::new());
//!
//!     // Render the frame to a byte buffer, then write it to the terminal.
//!     let out = frame.render(prev.as_ref(), cursor);
//!     driver.write_all(&out)?;
//!     driver.flush()?;
//!     prev = Some(frame);
//!
//!     // Both descriptors are non-blocking, so `poll` waits for readiness instead
//!     // of blocking on a read.
//!     let mut fds = [
//!         libc::pollfd { fd: driver.signal_fd(), events: libc::POLLIN, revents: 0 },
//!         libc::pollfd { fd: driver.input_fd(), events: libc::POLLIN, revents: 0 },
//!     ];
//!     let mut raw = [0u8; 256];
//!
//!     loop {
//!         if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) } < 0 {
//!             let err = std::io::Error::last_os_error();
//!             // `poll` is never restarted by `SA_RESTART`, so a SIGWINCH makes it
//!             // return `EINTR`. The handler writes the resize byte to the signal
//!             // pipe before returning, so retrying reports it as `POLLIN`.
//!             if err.kind() == std::io::ErrorKind::Interrupted {
//!                 continue;
//!             }
//!             return Err(err);
//!         }
//!
//!         // Handle a terminal resize.
//!         if fds[0].revents & libc::POLLIN != 0 {
//!             let new_size = driver.size()?;
//!             if new_size != size {
//!                 size = new_size;
//!                 let mut frame = tuinix::TerminalFrame::new(size);
//!                 write_text(&mut frame, "Welcome to tuinix!", title_style);
//!                 write_text(&mut frame, "\nPress any key ('q' to quit)", tuinix::TerminalStyle::new());
//!                 let out = frame.render(prev.as_ref(), cursor);
//!                 driver.write_all(&out)?;
//!                 driver.flush()?;
//!                 prev = Some(frame);
//!             }
//!         }
//!
//!         // Handle available input.
//!         if fds[1].revents & libc::POLLIN != 0 {
//!             while let Some(n) = tuinix::try_nonblocking(driver.read(&mut raw))? {
//!                 if n == 0 {
//!                     break;
//!                 }
//!                 input.feed(&raw[..n]);
//!                 while let Some(event) = input.next() {
//!                     let tuinix::TerminalInput::Key(key_input) = event else {
//!                         continue;  // Skip mouse events
//!                     };
//!
//!                     // Display the input
//!                     let mut frame = tuinix::TerminalFrame::new(size);
//!                     write_text(&mut frame, &format!("Key pressed: {:?}\n", key_input), tuinix::TerminalStyle::new());
//!                     write_text(&mut frame, "\nPress any key ('q' to quit)\n", tuinix::TerminalStyle::new());
//!                     let out = frame.render(prev.as_ref(), cursor);
//!                     driver.write_all(&out)?;
//!                     driver.flush()?;
//!                     prev = Some(frame);
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! A lone `ESC` byte is ambiguous: a terminal sends the same byte whether the
//! user pressed the Escape key or started a sequence such as `ESC [ A`. Waiting
//! for more input reports Escape only when the next key arrives, and the two
//! bytes then read as one Alt+key sequence. The fix is to poll with a short
//! timeout while [`InputStream::has_pending_escape()`] is `true`, and to commit
//! the byte with [`InputStream::resolve_escape()`] once the wait has elapsed.
//! Around 50 ms, the default of Vim's `ttimeoutlen`, is the usual choice.
//!
//! For a full example of an event loop driven with `poll`, and how to handle keyboard, mouse, and resize events together, see the [demo.rs] example.
//!
//! [demo.rs]: https://github.com/sile/tuinix/blob/main/examples/demo.rs
#![warn(missing_docs)]
use std::{io::ErrorKind, os::fd::RawFd};

mod frame;
mod geometry;
mod input;
mod style;
mod terminal;

pub use frame::{TerminalChar, TerminalFrame};
pub use geometry::{TerminalPosition, TerminalRegion, TerminalSize};
pub use input::{InputStream, KeyCode, KeyInput, MouseEvent, MouseInput, TerminalInput};
pub use style::{TerminalColor, TerminalStyle};
pub use terminal::TerminalDriver;

pub(crate) fn set_fd_nonblocking(fd: RawFd) -> std::io::Result<()> {
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
/// This utility function is designed to work with non-blocking I/O operations. The
/// input and signal file descriptors of [`TerminalDriver`] are non-blocking, so it
/// is useful when reading from them in an event loop. When a non-blocking operation returns a
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

/// Compiles the code examples in `README.md` as doctests so that they cannot
/// drift away from the API.
///
/// The example is marked `no_run`: it drives a real terminal and is only
/// type-checked.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
