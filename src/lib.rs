use std::{io::ErrorKind, os::fd::RawFd};

mod frame;
mod geometry;
pub mod input;
mod terminal;

pub use frame::{Rgb, TerminalChar, TerminalFrame, TerminalStyle};
pub use geometry::{TerminalPosition, TerminalSize};
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
