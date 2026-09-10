use std::{
    fs::File,
    io::{self, BufWriter, Error, IsTerminal, Read, Stdout, Write},
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
/// frame or input buffer, but it caches the most recently observed terminal size
/// so [`TerminalDriver::size()`] always reports a valid value. It owns the file
/// descriptors for input and output and the saved terminal modes, and it is
/// responsible for entering and leaving raw mode and the alternate screen. This
/// makes it a good target for implementing [`Read`] and [`Write`], so an
/// application can read raw bytes from the terminal
/// and write raw output bytes back to it, feeding those bytes in and out of an
/// [`InputStream`](crate::InputStream) and a [`TerminalFrame`](crate::TerminalFrame).
///
/// The input and signal file descriptors are non-blocking, so an application can
/// drive them from an external event loop without affecting the output side.
///
/// SIGWINCH is owned exclusively by the driver: creating one installs a handler
/// that replaces any handler the application had configured, and registers it
/// with `SA_RESTART` so the driver's own size query is not interrupted by a
/// resize. A `poll`-based event loop still observes `EINTR` (because `poll` is
/// never restarted), but the byte the handler writes to the signal pipe makes
/// the resize available on the next `poll`. Other signals are left untouched.
///
/// Only one instance can exist at a time; it is automatically restored to the
/// original terminal state when dropped.
pub struct TerminalDriver {
    input: File,
    output: BufWriter<Stdout>,
    signal: File,
    original_termios: libc::termios,
    cached_size: TerminalSize,
}

impl TerminalDriver {
    /// Creates a new terminal driver.
    ///
    /// This enters raw mode, switches to the alternate screen, hides the cursor,
    /// and installs a SIGWINCH handler (taking over any handler the application
    /// had configured). The input and signal file descriptors are made
    /// non-blocking. The initial terminal size is cached and can be read with
    /// [`TerminalDriver::size()`]; it is refreshed automatically whenever a resize
    /// notification arrives.
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

        // Open a fresh, independent, non-blocking description of the terminal
        // device that stdin is connected to. This keeps the original stdin (fd 0)
        // open for child processes and avoids making the output side non-blocking
        // through a shared open file description.
        let input_fd = open_nonblocking_input(stdin.as_raw_fd())?;
        let input = unsafe { File::from_raw_fd(input_fd) };
        let mut this = Self {
            input,
            output: BufWriter::new(stdout),
            signal: set_sigwinch_handler()?,
            original_termios,
            cached_size: TerminalSize::default(),
        };

        // The signal pipe does not share an open file description with the output,
        // so marking it non-blocking has no side effects on writes.
        crate::set_fd_nonblocking(this.signal.as_raw_fd(), true)?;

        // Seed the cached size with the current terminal dimensions.
        this.cached_size = this.query_terminal_size()?;

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
    /// The descriptor is a fresh, independent, non-blocking open of the terminal
    /// device that stdin is connected to. The original stdin (fd 0) is left
    /// untouched, and the descriptor is stable for the lifetime of the driver.
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
    /// The descriptor is non-blocking, so it can be monitored directly with an
    /// external event loop.
    pub fn signal_fd(&self) -> RawFd {
        self.signal.as_raw_fd()
    }

    /// Enables mouse input reporting in the terminal.
    ///
    /// Mouse events will be reported as raw bytes on the input stream, which the
    /// application feeds into [`InputStream::feed()`](crate::InputStream::feed)
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
    /// The driver caches the most recently observed size. This method first drains
    /// any pending resize notifications without blocking; if at least one was
    /// received it re-queries the terminal and updates the cache, so the returned
    /// value reflects the latest resize. If no notification is pending it returns
    /// the cached size immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal size cannot be re-queried after a resize
    /// notification.
    pub fn size(&mut self) -> io::Result<TerminalSize> {
        let mut notified = false;
        loop {
            match self.signal.read(&mut [0u8]) {
                Ok(_) => notified = true,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        if notified {
            self.cached_size = self.query_terminal_size()?;
        }
        Ok(self.cached_size)
    }

    fn query_terminal_size(&self) -> io::Result<TerminalSize> {
        let mut winsize = MaybeUninit::<libc::winsize>::zeroed();
        if unsafe { libc::ioctl(self.output_fd(), libc::TIOCGWINSZ, winsize.as_mut_ptr()) } == 0 {
            let winsize = unsafe { winsize.assume_init() };
            return Ok(TerminalSize {
                rows: winsize.ws_row as usize,
                cols: winsize.ws_col as usize,
            });
        }
        Err(Error::last_os_error())
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

/// Installs the SIGWINCH handler, taking over any handler the application may
/// already have configured.
///
/// SIGWINCH is owned exclusively by the driver for the lifetime of the
/// [`TerminalDriver`] instance. The handler writes one byte to a pipe so that an
/// external event loop can observe a resize without polling the terminal size.
///
/// `SA_RESTART` is set so the kernel automatically restarts the restartable
/// syscalls the driver performs (such as the ioctl that queries the terminal
/// size). Note that `poll`/`select` are never restarted, so an event loop that
/// waits with `poll` still sees `EINTR` for each SIGWINCH; since the handler
/// writes a byte to the signal pipe, that resize is picked up on the next
/// `poll`. Other signals are unaffected.
fn set_sigwinch_handler() -> io::Result<File> {
    let mut pipefd = [0 as RawFd; 2];
    check_libc_result(unsafe { libc::pipe(pipefd.as_mut_ptr()) })?;
    unsafe {
        SIGWINCH_PIPE_FD = pipefd[1];

        let mut sigaction = MaybeUninit::<libc::sigaction>::zeroed().assume_init();

        sigaction.sa_sigaction = handle_sigwinch as *const () as libc::sighandler_t;
        sigaction.sa_flags = libc::SA_RESTART;

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

        let mut terminal = TerminalDriver::new().expect("ok");

        // The cached size is seeded with the actual terminal dimensions.
        assert!(!terminal.size().expect("size").is_empty());

        // Creating a second driver should fail while the first one exists
        assert!(TerminalDriver::new().is_err());

        // After dropping the first driver, creating a new one should succeed
        std::mem::drop(terminal);
        assert!(TerminalDriver::new().is_ok());
    }
}
