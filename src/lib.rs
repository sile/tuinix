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
//! ## Where to look next
//!
//! - [`docs::input_decoding`] — the byte sequences [`InputDecoder`] recognizes,
//!   and what it does with the bytes that decode to nothing.
//! - [`docs::frame_writes`] — where a [`Frame`] stores characters, and what
//!   happens when a write does not fit.
//!
//! ## Architecture
//!
//! The library separates the *I/O* of a terminal from the *pure data* that an
//! application works with.
//!
//! - [`InputDecoder`] is a pure input parser. It accumulates raw bytes and yields
//!   parsed [`Input`] values, but it never performs I/O itself.
//! - [`Frame`] is a pure frame buffer. It renders itself into a byte
//!   buffer, comparing against a previous frame to redraw only what changed, and
//!   it never performs I/O itself.
//! - [`TerminalDriver`] owns the file descriptors and terminal modes. It is
//!   responsible for entering and leaving raw mode and the alternate screen, and
//!   it implements [`Read`](std::io::Read) and [`Write`](std::io::Write) so an
//!   application can read raw input bytes from the terminal and write raw output
//!   bytes back to it.
//!
//! The application is responsible for driving the loop: read raw bytes from the
//! driver, feed them into [`InputDecoder::feed()`], pull parsed
//! [`Input`] values out with [`InputDecoder::next()`], build a
//! [`Frame`], render it into a byte buffer with
//! [`Frame::render()`], and write that buffer to the driver.
//!
//! ## Basic Example
//!
//! This example demonstrates basic terminal UI functionality including initializing the terminal,
//! drawing styled text, processing keyboard events, and handling terminal resizing.
//!
//! ```no_run
//! use std::io::{Read, Write};
//! use std::num::NonZeroUsize;
//!
//! fn main() -> std::io::Result<()> {
//!     // Initialize terminal driver and query its size
//!     let mut driver = tuinix::TerminalDriver::new()?;
//!     let mut size = driver.size();
//!     let mut input = tuinix::InputDecoder::new();
//!     let cursor = None;
//!     let mut prev = None;
//!
//!     // Maps a non-blocking I/O result's `WouldBlock` to `Ok(None)`.
//!     fn would_block_as_none<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
//!         match result {
//!             Ok(v) => Ok(Some(v)),
//!             Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
//!             Err(e) => Err(e),
//!         }
//!     }
//!
//!     // NOTE: This is an ASCII-oriented demo helper: every character is assigned a width of 1.
//!     // Non-ASCII characters (for example CJK or emoji) would need the caller to supply their
//!     // actual width, because Frame does not compute character widths itself.
//!     //
//!     // The write position is the caller's to keep; it is threaded through `put_char`,
//!     // which returns the position just past each character.
//!     fn write_text(
//!         frame: &mut tuinix::Frame,
//!         at: tuinix::Position,
//!         text: &str,
//!         style: tuinix::Style,
//!     ) -> tuinix::Position {
//!         let mut at = at;
//!         for c in text.chars() {
//!             match c {
//!                 '\n' => at = at.next_line(),
//!                 // `next_tab_stop` takes a non-zero width; the application owns the value.
//!                 '\t' => at = at.next_tab_stop(NonZeroUsize::new(8).expect("8 is not 0")),
//!                 c if c.is_control() => {}
//!                 c => {
//!                     at = frame.put_char(at, tuinix::Char::new(c, 1, style).expect("valid char"));
//!                 }
//!             }
//!         }
//!         at
//!     }
//!
//!     // Add styled content to a frame
//!     let title_style = tuinix::Style::new().bold().fg_color(tuinix::Color::GREEN);
//!     let mut frame = tuinix::Frame::new(size);
//!     let mut at = tuinix::Position::ORIGIN;
//!     at = write_text(&mut frame, at, "Welcome to tuinix!", title_style);
//!     write_text(&mut frame, at, "\nPress any key ('q' to quit)", tuinix::Style::new());
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
//!         libc::pollfd { fd: driver.resize_signal_fd(), events: libc::POLLIN, revents: 0 },
//!         libc::pollfd { fd: driver.input_fd(), events: libc::POLLIN, revents: 0 },
//!     ];
//!     let mut raw = [0u8; 256];
//!
//!     // How long to wait for more input while a lone `ESC` is held.
//!     const ESCAPE_TIMEOUT_MS: i32 = 50;
//!
//!     loop {
//!         // A lone `ESC` byte is ambiguous: the terminal reports the Escape key
//!         // and the start of a sequence such as `ESC [ A` identically. While one
//!         // is held, wait only briefly so it is reported as Escape promptly
//!         // instead of sitting there until the next key arrives.
//!         let timeout = if input.has_uncommitted_escape() {
//!             ESCAPE_TIMEOUT_MS
//!         } else {
//!             -1
//!         };
//!         let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
//!         if n < 0 {
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
//!         // A descriptor that hung up or failed can never become ready again, so
//!         // stop instead of spinning on it.
//!         if fds
//!             .iter()
//!             .any(|fd| fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0)
//!         {
//!             return Err(std::io::Error::other("terminal closed"));
//!         }
//!
//!         if n == 0 {
//!             // The wait elapsed with the lone `ESC` still held, so commit it as
//!             // the Escape key.
//!             input.commit_escape();
//!         }
//!
//!         // Handle a terminal resize.
//!         if fds[0].revents & libc::POLLIN != 0 {
//!             driver.handle_resize_signal()?;
//!             let new_size = driver.size();
//!             if new_size != size {
//!                 size = new_size;
//!                 let mut frame = tuinix::Frame::new(size);
//!                 let mut at = tuinix::Position::ORIGIN;
//!                 at = write_text(&mut frame, at, "Welcome to tuinix!", title_style);
//!                 write_text(&mut frame, at, "\nPress any key ('q' to quit)", tuinix::Style::new());
//!                 let out = frame.render(prev.as_ref(), cursor);
//!                 driver.write_all(&out)?;
//!                 driver.flush()?;
//!                 prev = Some(frame);
//!             }
//!         }
//!
//!         // Handle available input.
//!         if fds[1].revents & libc::POLLIN != 0 {
//!             while let Some(n @ 1..) = would_block_as_none(driver.read(&mut raw))? {
//!                 input.feed(&raw[..n]);
//!             }
//!         }
//!
//!         // Drain every event the bytes fed above (or the committed `ESC`) made
//!         // available. This is the single place that consumes the decoder, so a
//!         // timeout and normal input share one path.
//!         while let Some(event) = input.next() {
//!             let tuinix::Input::Key(key_input) = event else {
//!                 continue;  // Skip mouse events
//!             };
//!
//!             // Display the input
//!             let mut frame = tuinix::Frame::new(size);
//!             let at = write_text(&mut frame, tuinix::Position::ORIGIN, &format!("Key pressed: {:?}\n", key_input), tuinix::Style::new());
//!             write_text(&mut frame, at, "\nPress any key ('q' to quit)\n", tuinix::Style::new());
//!             let out = frame.render(prev.as_ref(), cursor);
//!             driver.write_all(&out)?;
//!             driver.flush()?;
//!             prev = Some(frame);
//!         }
//!     }
//! }
//! ```
//!
//! The example waits with a short timeout while
//! [`InputDecoder::has_uncommitted_escape()`] is `true`, then calls
//! [`InputDecoder::commit_escape()`] once that wait has elapsed. Around 50 ms, the
//! default of Vim's `ttimeoutlen`, is the usual choice; the constant above and the
//! `n == 0` branch are that rule.
//!
//! For a full example of an event loop driven with `poll`, and how to handle keyboard, mouse, and resize events together, see the [demo.rs] example.
//!
//! [demo.rs]: https://github.com/sile/tuinix/blob/main/examples/demo.rs
#![warn(missing_docs)]
#![deny(unsafe_code)]

mod frame;
mod geometry;
mod input;
mod style;
mod terminal;

/// Supplemental documentation for tuinix's design.
///
/// These pages go deeper than the item-level documentation: they gather the
/// behavior of one area in one place, where rustdoc's per-item form makes it
/// hard to see the whole.
pub mod docs {
    /// Reference for what [`InputDecoder`](crate::InputDecoder) recognizes:
    /// the byte sequences that become an [`Input`](crate::Input), and what
    /// happens to the bytes that become nothing.
    #[doc = include_str!("../docs/input-decoding.md")]
    pub mod input_decoding {}

    /// How a [`Frame`](crate::Frame) stores characters: where a write goes,
    /// what happens when it does not fit, and what a position means.
    #[doc = include_str!("../docs/frame-writes.md")]
    pub mod frame_writes {}
}

pub use frame::{Char, Frame};
pub use geometry::{Position, Region, Size};
pub use input::{Input, InputDecoder, KeyCode, KeyInput, MouseInput, MouseInputKind};
pub use style::{Color, Style};
pub use terminal::TerminalDriver;

/// Compiles the code examples in `README.md` as doctests so that they cannot
/// drift away from the API.
///
/// The example is marked `no_run`: it drives a real terminal and is only
/// type-checked.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
