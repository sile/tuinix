use std::{
    fs::File,
    io::{self, BufWriter, Error, ErrorKind, IsTerminal, Read, Stdout, Write},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, RawFd},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::TerminalSize;

static TERMINAL_EXISTS: AtomicBool = AtomicBool::new(false);

static mut SIGWINCH_PIPE_FD: RawFd = 0;

/// Thin TTY driver that owns the terminal file descriptors and terminal modes.
///
/// This type performs no high-level state keeping: it does not store the current
/// frame, size, or input buffer. It owns the file descriptors for input and output
/// and the saved terminal modes, and it is responsible for entering and leaving
/// raw mode and the alternate screen. This makes it a good target for implementing
/// [`Read`] and [`Write`], so an application can read raw bytes from the terminal
/// and write raw output bytes back to it, feeding those bytes in and out of a
/// [`TerminalState`](crate::TerminalState).
///
/// Only one instance can exist at a time; it is automatically restored to the
/// original terminal state when dropped.
pub struct TerminalDriver {
    input: File,
    output: BufWriter<Stdout>,
    signal: File,
    original_termios: libc::termios,
    input_replaced: bool,
}

impl TerminalDriver {
    /// Creates a new terminal driver.
    ///
    /// This enters raw mode, switches to the alternate screen, hides the cursor,
    /// and installs a SIGWINCH handler. Use [`TerminalDriver::size()`] to query the
    /// initial terminal size.
    ///
    /// # Errors
    ///
    /// Returns an error if another driver instance already exists, if stdin or
    /// stdout is not a terminal, or if a terminal configuration call fails.
    pub fn new() -> io::Result<Self> {
        if TERMINAL_EXISTS.swap(true, Ordering::SeqCst) {
            return Err(Error::other("TerminalDriver instance already exists"));
        }

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        if !stdin.is_terminal() {
            return Err(Error::other("STDIN is not a terminal"));
        }
        if !stdout.is_terminal() {
            return Err(Error::other("STDOUT is not a terminal"));
        }

        let mut termios = MaybeUninit::<libc::termios>::zeroed();
        check_libc_result(unsafe { libc::tcgetattr(stdin.as_raw_fd(), termios.as_mut_ptr()) })?;
        let original_termios = unsafe { termios.assume_init() };

        let input_tty_path = unsafe {
            let mut path = [0u8; libc::PATH_MAX as usize];
            if libc::ttyname_r(stdin.as_raw_fd(), path.as_mut_ptr().cast(), path.len()) != 0 {
                None
            } else {
                Some(path)
            }
        };

        // Own a duplicate of the stdin fd instead of fd 0 itself, so that the
        // original stdin stays open (e.g. for use by child processes) even
        // after this driver is dropped or the input fd is replaced.
        let stdin_fd = unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if stdin_fd < 0 {
            return Err(Error::last_os_error());
        }
        let stdin = unsafe { File::from_raw_fd(stdin_fd) };
        let mut this = Self {
            input: stdin,
            output: BufWriter::new(stdout),
            signal: set_sigwinch_handler()?,
            original_termios,
            input_replaced: false,
        };
        this.enable_raw_mode()?;
        this.enable_alternate_screen()?;
        this.hide_cursor()?;
        this.output.flush()?;

        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Disable alternate screen and raw mode to show the panic message.
            // `tcsetattr` operates on the terminal device, so a fresh open is
            // enough to restore the terminal state regardless of which fd is
            // currently used for the input.
            let mut stdout = std::io::stdout();
            let default_path = b"/dev/tty\0";
            let path = match &input_tty_path {
                Some(p) => p.as_slice(),
                None => default_path,
            };
            let fd = unsafe { libc::open(path.as_ptr().cast(), libc::O_RDONLY | libc::O_NOCTTY) };
            if fd >= 0 {
                unsafe {
                    libc::tcsetattr(fd, libc::TCSAFLUSH, &original_termios);
                    libc::close(fd);
                }
            }
            let _ = write!(stdout, "\x1b[?1049l");
            let _ = stdout.flush();

            // Call the default panic handler
            default_hook(panic_info);
        }));

        Ok(this)
    }

    /// Returns the input file descriptor.
    ///
    /// The returned descriptor is a duplicate of the stdin file descriptor (the
    /// original stdin, fd 0, is left untouched). The value changes after
    /// [`TerminalDriver::set_input_nonblocking()`] is called; fetch it again in that
    /// case.
    pub fn input_fd(&self) -> RawFd {
        self.input.as_raw_fd()
    }

    /// Returns the output file descriptor.
    pub fn output_fd(&self) -> RawFd {
        self.output.get_ref().as_raw_fd()
    }

    /// Returns the file descriptor that receives terminal resize signal
    /// notifications.
    ///
    /// Make it non-blocking with [`TerminalDriver::set_signal_nonblocking()`] when
    /// using external event loops.
    pub fn signal_fd(&self) -> RawFd {
        self.signal.as_raw_fd()
    }

    /// Makes the terminal resize signal file descriptor non-blocking, and returns
    /// the file descriptor.
    ///
    /// The signal fd is a pipe that does not share an open file description with
    /// the output, so making it non-blocking has no side effects on writes. This is
    /// required when combining [`TerminalDriver::poll_resize()`] with external event
    /// loops.
    pub fn set_signal_nonblocking(&mut self) -> io::Result<RawFd> {
        let fd = self.signal_fd();
        crate::set_fd_nonblocking(fd, true)?;
        Ok(fd)
    }

    /// Enables mouse input reporting in the terminal.
    ///
    /// Mouse events will be reported as raw bytes on the input stream, which the
    /// application feeds into [`TerminalState::feed_bytes()`](crate::TerminalState::feed_bytes)
    /// so they parse as [`TerminalInput::Mouse`](crate::TerminalInput::Mouse) values.
    pub fn enable_mouse_input(&mut self) -> io::Result<()> {
        // Enable mouse reporting in SGR mode (more reliable than X10/X11 mode)
        write!(self.output, "\x1b[?1000h")?; // Enable basic mouse reporting
        write!(self.output, "\x1b[?1002h")?; // Enable button event tracking and motion
        write!(self.output, "\x1b[?1015h")?; // Enable urxvt extended coordinate reporting
        write!(self.output, "\x1b[?1006h")?; // Enable SGR extended coordinate reporting
        self.output.flush()?;
        Ok(())
    }

    /// Disables mouse input reporting in the terminal.
    ///
    /// This method disables all mouse event reporting that was previously enabled
    /// with [`TerminalDriver::enable_mouse_input()`].
    pub fn disable_mouse_input(&mut self) -> io::Result<()> {
        // Disable mouse reporting (reverse order)
        write!(self.output, "\x1b[?1006l")?; // Disable SGR extended coordinate reporting
        write!(self.output, "\x1b[?1015l")?; // Disable urxvt extended coordinate reporting
        write!(self.output, "\x1b[?1002l")?; // Disable button event tracking
        write!(self.output, "\x1b[?1000l")?; // Disable basic mouse reporting
        self.output.flush()?;
        Ok(())
    }

    /// Returns the current terminal size.
    ///
    /// This queries the terminal for its current dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal size cannot be queried.
    pub fn size(&self) -> io::Result<TerminalSize> {
        self.resize()
    }

    /// Waits for a terminal resize event to occur and returns the new terminal size.
    ///
    /// By default, this method blocks until a resize occurs. To use it in
    /// non-blocking mode, first call [`TerminalDriver::set_signal_nonblocking()`].
    /// Unlike the input fd, the signal fd is a pipe that does not share an open file
    /// description with the output, so making it non-blocking has no side effects on
    /// writes.
    pub fn poll_resize(&mut self) -> io::Result<TerminalSize> {
        self.signal.read_exact(&mut [0])?;
        self.resize()
    }

    /// Makes the terminal input non-blocking by replacing the input file descriptor
    /// with a fresh open of the terminal device that stdin is connected to, and
    /// returns the new file descriptor.
    ///
    /// This is the recommended way to make the input non-blocking (e.g. for use with
    /// `mio` or `tokio::io::unix::AsyncFd`). Making the input fd non-blocking
    /// directly (e.g. via `fcntl` with `O_NONBLOCK`) also affects the output fd,
    /// because both share an open file description in typical interactive terminals,
    /// which may cause writes to fail with `EAGAIN` / `EWOULDBLOCK`. This method
    /// avoids that by opening a fresh, independent file description for the input.
    ///
    /// If the input fd has already been made non-blocking, the `O_NONBLOCK` flag is
    /// cleared from the original file description as part of this method (also on
    /// failure), so writes stop failing with `EAGAIN` after this method returns.
    ///
    /// # Errors
    ///
    /// Returns an error if the input has already been replaced, if the terminal
    /// device that stdin is connected to cannot be identified or opened, or if
    /// configuring the new file descriptor fails. On error, the input is not made
    /// non-blocking and the input file descriptor stays in use.
    pub fn set_input_nonblocking(&mut self) -> io::Result<RawFd> {
        if self.input_replaced {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Input fd has already been replaced",
            ));
        }

        let fd = match open_nonblocking_input(self.input_fd()) {
            Ok(fd) => fd,
            Err(err) => {
                let _ = self.clear_input_nonblocking();
                return Err(err);
            }
        };

        if let Err(err) = self.clear_input_nonblocking() {
            unsafe { libc::close(fd) };
            return Err(err);
        }

        self.input = unsafe { File::from_raw_fd(fd) };
        self.input_replaced = true;
        Ok(fd)
    }

    fn clear_input_nonblocking(&self) -> io::Result<()> {
        crate::set_fd_nonblocking(self.input_fd(), false)
    }

    fn resize(&self) -> io::Result<TerminalSize> {
        let mut winsize = MaybeUninit::<libc::winsize>::zeroed();
        check_libc_result(unsafe {
            libc::ioctl(self.output_fd(), libc::TIOCGWINSZ, winsize.as_mut_ptr())
        })?;

        let winsize = unsafe { winsize.assume_init() };
        Ok(TerminalSize {
            rows: winsize.ws_row as usize,
            cols: winsize.ws_col as usize,
        })
    }

    fn enable_alternate_screen(&mut self) -> io::Result<()> {
        write!(self.output, "\x1b[?1049h")
    }

    fn disable_alternate_screen(&mut self) -> io::Result<()> {
        write!(self.output, "\x1b[?1049l")
    }

    fn enable_raw_mode(&mut self) -> io::Result<()> {
        let mut raw = self.original_termios;

        // Input modes: no break, no CR to NL, no parity check, no strip char,
        // no start/stop output control.
        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);

        // Output modes - disable post processing
        raw.c_oflag &= !libc::OPOST;

        // Control modes - clear size bits, parity checking off, set 8 bit chars
        raw.c_cflag &= !(libc::CSIZE | libc::PARENB);
        raw.c_cflag |= libc::CS8;

        // Local modes - disable echoing, canonical mode, signal chars, and extended features
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);

        // 1 byte at a time, no timer
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        check_libc_result(unsafe { libc::tcsetattr(self.input_fd(), libc::TCSAFLUSH, &raw) })?;

        Ok(())
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        check_libc_result(unsafe {
            libc::tcsetattr(self.input_fd(), libc::TCSAFLUSH, &self.original_termios)
        })?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        write!(self.output, "\x1b[?25l")
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        write!(self.output, "\x1b[?25h")
    }
}

impl Read for TerminalDriver {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for TerminalDriver {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.disable_mouse_input();
        let _ = self.disable_alternate_screen();
        let _ = self.disable_raw_mode();
        let _ = self.show_cursor();
        let _ = self.output.flush();
        unsafe { libc::close(SIGWINCH_PIPE_FD) };
        TERMINAL_EXISTS.store(false, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for TerminalDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalDriver").finish()
    }
}

fn check_libc_result(result: libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}

unsafe extern "C" fn handle_sigwinch(_: libc::c_int) {
    unsafe {
        let _ = libc::write(SIGWINCH_PIPE_FD, [0].as_ptr().cast(), 1);
    }
}

fn set_sigwinch_handler() -> io::Result<File> {
    let mut pipefd = [0 as RawFd; 2];
    check_libc_result(unsafe { libc::pipe(pipefd.as_mut_ptr()) })?;
    unsafe {
        SIGWINCH_PIPE_FD = pipefd[1];

        let mut sigaction = MaybeUninit::<libc::sigaction>::zeroed().assume_init();

        sigaction.sa_sigaction = handle_sigwinch as *const () as libc::sighandler_t;
        sigaction.sa_flags = 0;

        check_libc_result(libc::sigemptyset(&mut sigaction.sa_mask))?;
        check_libc_result(libc::sigaction(
            libc::SIGWINCH,
            &sigaction,
            std::ptr::null_mut(),
        ))?;
        Ok(File::from_raw_fd(pipefd[0]))
    }
}

/// Opens a fresh, independent file description of the terminal device that
/// `input_fd` is connected to, and makes it non-blocking. The original file
/// description is not modified.
fn open_nonblocking_input(input_fd: RawFd) -> io::Result<RawFd> {
    let mut path = [0u8; libc::PATH_MAX as usize];
    if unsafe { libc::ttyname_r(input_fd, path.as_mut_ptr().cast(), path.len()) } != 0 {
        return Err(Error::last_os_error());
    }
    let fd = unsafe {
        libc::open(
            path.as_ptr().cast(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOCTTY,
        )
    };
    if fd < 0 {
        return Err(Error::last_os_error());
    }
    if let Err(err) = crate::set_fd_nonblocking(fd, true) {
        unsafe { libc::close(fd) };
        return Err(err);
    }
    Ok(fd)
}

#[cfg(test)]
mod tests {
    use std::io::IsTerminal;
    use std::os::fd::RawFd;

    use super::{TerminalDriver, open_nonblocking_input};

    #[test]
    fn open_nonblocking_input_rejects_non_tty() {
        let mut pipefd = [0 as RawFd; 2];
        assert_eq!(unsafe { libc::pipe(pipefd.as_mut_ptr()) }, 0);
        let result = open_nonblocking_input(pipefd[0]);
        unsafe {
            libc::close(pipefd[0]);
            libc::close(pipefd[1]);
        }
        assert!(result.is_err());
    }

    #[test]
    fn open_nonblocking_input_opens_fresh_description() {
        let mut master = 0;
        let mut slave = 0;
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );

        let fd = open_nonblocking_input(slave).expect("ok");
        assert_ne!(fd, slave);
        assert_eq!(unsafe { libc::isatty(fd) }, 1);

        // The new fd is non-blocking while the original one is left untouched.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        assert!(flags & libc::O_NONBLOCK != 0);
        let flags = unsafe { libc::fcntl(slave, libc::F_GETFL, 0) };
        assert!(flags & libc::O_NONBLOCK == 0);

        // Data written to the master is readable from the new fd.
        assert_eq!(
            unsafe { libc::write(master, b"hi\n".as_ptr().cast(), 3) },
            3
        );
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut pfd, 1, 2000) }, 1);
        let mut buf = [0u8; 16];
        assert_eq!(unsafe { libc::read(fd, buf.as_mut_ptr().cast(), 16) }, 3);
        assert_eq!(&buf[..3], b"hi\n");

        unsafe {
            libc::close(fd);
            libc::close(master);
            libc::close(slave);
        }
    }

    #[test]
    fn duplicate_check() {
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            return;
        }

        let terminal = TerminalDriver::new().expect("ok");

        // Creating a second driver should fail while the first one exists
        assert!(TerminalDriver::new().is_err());

        // After dropping the first driver, creating a new one should succeed
        std::mem::drop(terminal);
        assert!(TerminalDriver::new().is_ok());
    }
}
