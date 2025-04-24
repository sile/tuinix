use std::os::fd::RawFd;

pub mod frame;
pub mod input;
pub mod terminal;

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

pub fn would_block<T>(result: std::io::Result<T>) -> std::io::Result<Option<T>> {
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
        Ok(v) => Ok(Some(v)),
    }
}
