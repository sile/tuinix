use std::{
    fs::File,
    io::{BufWriter, Error, ErrorKind, IsTerminal, Read, Stdout, Write},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, RawFd},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{
    TerminalFrame, TerminalPosition, TerminalSize,
    input::{InputReader, TerminalInput},
};

static TERMINAL_EXISTS: AtomicBool = AtomicBool::new(false);

static mut SIGWINCH_PIPE_FD: RawFd = 0;

/// Terminal interface for building TUI (Terminal User Interface) applications.
///
/// The [`Terminal`] struct provides a foundational layer for creating terminal-based
/// user interfaces by managing:
///
/// - Raw terminal mode configuration
/// - Alternate screen buffer
/// - Terminal size detection and window resize events
/// - Input event handling
/// - Cursor positioning and visibility
/// - Drawing frames with styled characters
///
/// Only one instance of [`Terminal`] can exist at a time, ensuring proper management
/// of terminal state. The terminal is automatically restored to its original state
/// when the [`Terminal`] instance is dropped.
///
/// # Basic Example
///
/// This example demonstrates the essential steps to initialize a terminal, create a frame,
/// draw it to the screen, and handle input events with a timeout.
///
/// ```no_run
/// use tuinix::{Terminal, TerminalFrame, TerminalSize};
/// use std::time::Duration;
///
/// fn main() -> std::io::Result<()> {
///     let mut terminal = Terminal::new()?;
///     let size = terminal.size();
///
///     // Create and draw a frame
///     let mut frame: TerminalFrame = TerminalFrame::new(size);
///     // Add content to frame...
///     terminal.draw(frame)?;
///
///     // Wait for events with timeout
///     let timeout = Duration::from_millis(100);
///     if let Some(event) = terminal.poll_event(&[], &[], Some(timeout))? {
///         // Handle input or resize events
///         println!("Received event: {:?}", event);
///     }
///
///     Ok(())
/// }
/// ```
///
/// # Non-blocking I/O Example
///
/// This example demonstrates how to use the terminal with non-blocking I/O operations
/// through the `mio` crate. This approach allows handling terminal events without
/// blocking the main thread, which is useful for responsive UIs or when integrating
/// with other event sources.
///
/// ```no_run
/// use std::time::Duration;
///
/// use mio::{Events, Interest, Poll, Token};
/// use tuinix::{Terminal, TerminalFrame, try_nonblocking, try_uninterrupted};
///
/// fn main() -> std::io::Result<()> {
///     // Initialize terminal
///     let mut terminal = Terminal::new()?;
///
///     // Create mio Poll instance
///     let mut poll = Poll::new()?;
///     let mut events = Events::with_capacity(10);
///
///     // Get file descriptors and set to non-blocking mode
///     let stdin_fd = terminal.set_input_nonblocking()?;
///     let signal_fd = terminal.set_signal_nonblocking()?;
///
///     // Register with mio poll
///     poll.registry().register(
///         &mut mio::unix::SourceFd(&stdin_fd),
///         Token(0),
///         Interest::READABLE
///     )?;
///     poll.registry().register(
///         &mut mio::unix::SourceFd(&signal_fd),
///         Token(1),
///         Interest::READABLE
///     )?;
///
///     // Event loop
///     loop {
///         // Wait for events with timeout
///         let timeout = Duration::from_millis(100);
///         if try_uninterrupted(poll.poll(&mut events, Some(timeout)))?.is_none() {
///             continue;
///         }
///
///         for event in events.iter() {
///             match event.token() {
///                 Token(0) => {
///                     // Handle input without blocking
///                     while let Some(input) = try_nonblocking(terminal.read_input())? {
///                         // Process input event
///                     }
///                 },
///                 Token(1) => {
///                     // Handle terminal resize without blocking
///                     while let Some(size) = try_nonblocking(terminal.wait_for_resize())? {
///                         // Terminal was resized, update UI
///                     }
///                 },
///                 _ => unreachable!(),
///             }
///         }
///
///         // Update display if needed
///     }
/// }
/// ```
pub struct Terminal {
    input: InputReader<File>,
    output: BufWriter<Stdout>,
    signal: File,
    original_termios: libc::termios,
    size: TerminalSize,
    last_frame: TerminalFrame,
    cursor: Option<TerminalPosition>,
    input_replaced: bool,
}

impl Terminal {
    /// Creates a new terminal interface with raw mode, alternate screen, and hidden cursor.
    ///
    /// This function initializes a terminal for TUI (Terminal User Interface) applications
    /// by:
    ///
    /// - Ensuring only one terminal instance exists at a time
    /// - Verifying stdin/stdout are connected to a terminal
    /// - Saving the original terminal state (restored on drop)
    /// - Enabling raw mode (for direct character-by-character input)
    /// - Switching to the alternate screen buffer
    /// - Hiding the cursor
    /// - Installing a SIGWINCH signal handler to detect terminal resize events
    /// - Installing a panic handler to restore terminal state on panic
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Another [`Terminal`] instance already exists
    /// - Standard input is not a terminal
    /// - Standard output is not a terminal
    /// - Terminal configuration fails
    pub fn new() -> std::io::Result<Self> {
        if TERMINAL_EXISTS.swap(true, Ordering::SeqCst) {
            return Err(Error::other("Terminal instance already exists"));
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
        // after this `Terminal` is dropped or the input fd is replaced.
        let stdin_fd = unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if stdin_fd < 0 {
            return Err(Error::last_os_error());
        }
        let stdin = unsafe { File::from_raw_fd(stdin_fd) };
        let mut this = Self {
            input: InputReader::new(stdin),
            output: BufWriter::new(stdout),
            signal: set_sigwinch_handler()?,
            original_termios,
            size: TerminalSize::EMPTY,
            last_frame: TerminalFrame::default(),
            cursor: None,
            input_replaced: false,
        };
        this.update_size()?;
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

    /// Returns the current terminal size.
    ///
    /// The size is updated when terminal resize events are detected through
    /// [`Terminal::wait_for_resize()`] or [`Terminal::poll_event()`].
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Returns the file descriptor of the terminal input.
    ///
    /// The returned descriptor is a duplicate of the stdin file descriptor (the original stdin,
    /// fd 0, is left untouched). The value changes after [`Terminal::set_input_nonblocking()`]
    /// is called; fetch it again in that case.
    pub fn input_fd(&self) -> RawFd {
        self.input.inner().as_raw_fd()
    }

    /// Returns the file descriptor of the terminal output.
    pub fn output_fd(&self) -> RawFd {
        self.output.get_ref().as_raw_fd()
    }

    /// Returns the file descriptor that receives terminal resize signal notifications.
    ///
    /// Make it non-blocking with [`Terminal::set_signal_nonblocking()`] when using external
    /// event loops.
    pub fn signal_fd(&self) -> RawFd {
        self.signal.as_raw_fd()
    }

    /// Makes the terminal resize signal file descriptor non-blocking, and returns the file
    /// descriptor.
    ///
    /// The signal fd is a pipe that does not share an open file description with the output, so
    /// making it non-blocking has no side effects on [`Terminal::draw()`]. This is required when
    /// combining [`Terminal::wait_for_resize()`] with external event loops.
    pub fn set_signal_nonblocking(&mut self) -> std::io::Result<RawFd> {
        let fd = self.signal_fd();
        crate::set_fd_nonblocking(fd, true)?;
        Ok(fd)
    }

    /// Enables mouse input reporting in the terminal.
    ///
    /// Mouse events will be received through [`Terminal::poll_event()`] or [`Terminal::read_input()`]
    /// as [`TerminalInput::Mouse`] variants.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tuinix::{Terminal, TerminalEvent, TerminalInput};
    /// use std::time::Duration;
    ///
    /// let mut terminal = Terminal::new()?;
    /// terminal.enable_mouse_input()?;
    ///
    /// loop {
    ///     if let Some(event) = terminal.poll_event(&[], &[], Some(Duration::from_millis(100)))? {
    ///         match event {
    ///             TerminalEvent::Input(TerminalInput::Mouse(mouse)) => {
    ///                 println!("Mouse event: {:?} at ({}, {})",
    ///                          mouse.event, mouse.position.col, mouse.position.row);
    ///             }
    ///             _ => {}
    ///         }
    ///     }
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn enable_mouse_input(&mut self) -> std::io::Result<()> {
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
    /// with [`Terminal::enable_mouse_input()`]. After calling this method, mouse
    /// events will no longer be sent to the application.
    ///
    /// Mouse input is automatically disabled when the Terminal is dropped, so calling
    /// this method manually is only necessary if you want to disable mouse input
    /// while keeping the Terminal instance active.
    pub fn disable_mouse_input(&mut self) -> std::io::Result<()> {
        // Disable mouse reporting (reverse order)
        write!(self.output, "\x1b[?1006l")?; // Disable SGR extended coordinate reporting
        write!(self.output, "\x1b[?1015l")?; // Disable urxvt extended coordinate reporting
        write!(self.output, "\x1b[?1002l")?; // Disable button event tracking
        write!(self.output, "\x1b[?1000l")?; // Disable basic mouse reporting
        self.output.flush()?;
        Ok(())
    }

    /// Waits for and returns the next terminal event.
    ///
    /// This method efficiently waits for either input events, terminal resize events,
    /// or custom file descriptor events using [`libc::select()`].
    ///
    /// If you want to use I/O polling mechanisms other than [`libc::select()`],
    /// please use the following methods directly:
    /// - [`Terminal::input_fd()`] and [`Terminal::read_input()`] for input events (call
    ///   [`Terminal::set_input_nonblocking()`] first, as required by external event loops)
    /// - [`Terminal::signal_fd()`] and [`Terminal::wait_for_resize()`] for resize events (call
    ///   [`Terminal::set_signal_nonblocking()`] first, as required by external event loops)
    ///
    /// # Parameters
    ///
    /// - `additional_readfds`: Additional file descriptors to monitor for read readiness
    /// - `additional_writefds`: Additional file descriptors to monitor for write readiness
    /// - `timeout`: Optional timeout duration; `None` blocks indefinitely
    ///
    /// # Returns
    ///
    /// - `Ok(Some(TerminalEvent))` if an input, resize, or file descriptor event was received
    /// - `Ok(None)` if the timeout expired without any event
    /// - `Err(e)` if an I/O error occurred
    pub fn poll_event(
        &mut self,
        additional_readfds: &[RawFd],
        additional_writefds: &[RawFd],
        timeout: Option<Duration>,
    ) -> std::io::Result<Option<TerminalEvent>> {
        if let Some(input) = self.input.read_input_from_buf()? {
            return Ok(Some(TerminalEvent::Input(input)));
        }

        let start_time = Instant::now();
        loop {
            unsafe {
                // Always monitor input and signal fds
                let mut readfds = MaybeUninit::<libc::fd_set>::zeroed();
                libc::FD_ZERO(readfds.as_mut_ptr());
                libc::FD_SET(self.input_fd(), readfds.as_mut_ptr());
                libc::FD_SET(self.signal_fd(), readfds.as_mut_ptr());
                let mut maxfd = self.input_fd().max(self.signal.as_raw_fd());

                // Add extra read fds
                for &fd in additional_readfds {
                    libc::FD_SET(fd, readfds.as_mut_ptr());
                    maxfd = maxfd.max(fd);
                }
                let mut readfds = readfds.assume_init();

                // Add extra write fds
                let mut writefds = MaybeUninit::<libc::fd_set>::zeroed();
                if !additional_writefds.is_empty() {
                    libc::FD_ZERO(writefds.as_mut_ptr());
                    for &fd in additional_writefds {
                        libc::FD_SET(fd, writefds.as_mut_ptr());
                        maxfd = maxfd.max(fd);
                    }
                }
                let mut writefds = writefds.assume_init();

                let mut timeval = MaybeUninit::<libc::timeval>::zeroed();
                let timeval_ptr = if let Some(duration) = timeout {
                    let duration = duration.saturating_sub(start_time.elapsed());
                    let tv = timeval.as_mut_ptr();
                    (*tv).tv_sec = duration.as_secs() as libc::time_t;
                    (*tv).tv_usec = duration.subsec_micros() as libc::suseconds_t;
                    tv
                } else {
                    std::ptr::null_mut()
                };

                let ret = libc::select(
                    maxfd + 1,
                    &mut readfds,
                    if additional_writefds.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        &mut writefds
                    },
                    std::ptr::null_mut(),
                    timeval_ptr,
                );

                if ret == -1 {
                    let e = Error::last_os_error();
                    if e.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                } else if ret == 0 {
                    // Timeout
                    return Ok(None);
                }

                // Check built-in fds first
                if libc::FD_ISSET(self.input_fd(), &readfds)
                    && let Some(input) = self.read_input()?
                {
                    return Ok(Some(TerminalEvent::Input(input)));
                }
                if libc::FD_ISSET(self.signal_fd(), &readfds) {
                    return self.wait_for_resize().map(TerminalEvent::Resize).map(Some);
                }

                // Check extra read fds
                for &fd in additional_readfds {
                    if libc::FD_ISSET(fd, &readfds) {
                        let readable = true;
                        return Ok(Some(TerminalEvent::FdReady { fd, readable }));
                    }
                }

                // Check extra write fds
                for &fd in additional_writefds {
                    if libc::FD_ISSET(fd, &writefds) {
                        let readable = false;
                        return Ok(Some(TerminalEvent::FdReady { fd, readable }));
                    }
                }
            }
        }
    }

    /// Reads and processes the next input event from the terminal.
    ///
    /// This method attempts to read raw bytes from the terminal input and parse them into a
    /// structured [`TerminalInput`] event.
    ///
    /// By default, this method blocks until input is available. To use it in non-blocking
    /// mode, first call [`Terminal::set_input_nonblocking()`].
    ///
    /// Note that an incomplete escape sequence (e.g. a lone `ESC` byte) stays in the internal
    /// buffer and is reported as `Ok(None)` until the rest of the sequence arrives. With a
    /// non-blocking input, a standalone `ESC` key event is therefore only reported once the
    /// following key is pressed, and may then be interpreted as an `Alt` key combination.
    ///
    /// While [`Terminal::poll_event()`] is generally recommended for receiving terminal input events,
    /// you may need to call this method directly when using external I/O polling crates like `mio`.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(input))` if an input event was successfully read and parsed
    /// - `Ok(None)` if not enough bytes were available to form a complete input event
    /// - `Err(e)` if an I/O error occurred while reading from the input
    ///
    /// # Errors
    ///
    /// This method returns an error if reading from the terminal input fails or encounters EOF.
    pub fn read_input(&mut self) -> std::io::Result<Option<TerminalInput>> {
        self.input.read_input()
    }

    /// Makes the terminal input non-blocking by replacing the input file descriptor with a fresh
    /// open of the terminal device that stdin is connected to, and returns the new file
    /// descriptor.
    ///
    /// This is the recommended way to make the input non-blocking (e.g. for use with `mio` or
    /// `tokio::io::unix::AsyncFd`). Making the input fd non-blocking directly (e.g. via `fcntl`
    /// with `O_NONBLOCK`) also affects the output fd, because both share an open file description
    /// in typical interactive terminals, which may cause [`Terminal::draw()`] to fail with
    /// `EAGAIN` / `EWOULDBLOCK`. This method avoids that by opening a fresh, independent file
    /// description for the input.
    ///
    /// If the input fd has already been made non-blocking, the `O_NONBLOCK` flag is cleared from
    /// the original file description as part of this method (also on failure), so `draw()` stops
    /// failing with `EAGAIN` after this method returns.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The input has already been replaced (the previously returned file descriptor stays
    ///   valid, since the `Terminal` keeps owning it)
    /// - The terminal device that stdin is connected to cannot be identified or opened
    /// - Configuring the new file descriptor fails
    ///
    /// On error, the input is not made non-blocking and the input file descriptor stays in use.
    ///
    /// # Notes
    ///
    /// - The returned file descriptor is owned by the `Terminal` and is closed when the `Terminal`
    ///   is dropped. `tokio::io::unix::AsyncFd` caches the file descriptor number it is given, so
    ///   if the number is reused after the `Terminal` closes it, the `AsyncFd` keeps monitoring an
    ///   unrelated file descriptor. Pass a duplicated file descriptor to wrappers that take
    ///   ownership rather than the returned one. `mio::unix::SourceFd(&fd)` borrows the descriptor
    ///   and can be used directly.
    /// - Call this method before registering the file descriptor with an external event loop
    ///   (e.g. `mio` or `AsyncFd`), because [`Terminal::input_fd()`] changes after the
    ///   replacement. Buffered input is preserved, so calling [`Terminal::read_input()`] beforehand
    ///   does not lose data.
    /// - Call this method only once per `Terminal`. Subsequent calls return `Err` and leave the
    ///   previously returned file descriptor valid.
    /// - The device path is opened directly instead of `/dev/tty`: on macOS, kqueue-based event
    ///   loops (e.g. `mio` / `tokio::io::unix::AsyncFd`) cannot monitor file descriptors obtained
    ///   from `/dev/tty`, whereas they work on the underlying device node. If the device path
    ///   cannot be determined or opened, running [`Terminal::poll_event()`] in a `spawn_blocking`
    ///   task is an alternative.
    /// - The original stdin file descriptor (fd 0) is left untouched, so `std::io::stdin()` and
    ///   child processes keep working as usual.
    pub fn set_input_nonblocking(&mut self) -> std::io::Result<RawFd> {
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

        self.input.replace_inner(unsafe { File::from_raw_fd(fd) });
        self.input_replaced = true;
        Ok(fd)
    }

    fn clear_input_nonblocking(&self) -> std::io::Result<()> {
        crate::set_fd_nonblocking(self.input_fd(), false)
    }

    /// Waits for a terminal resize event to occur and returns the new terminal size.
    ///
    /// By default, this method blocks until input is available. To use it in non-blocking
    /// mode, first call [`Terminal::set_signal_nonblocking()`].
    /// Unlike the input fd, the signal fd is a pipe that does not share an open file description
    /// with the output, so making it non-blocking has no side effects on [`Terminal::draw()`].
    ///
    /// While [`Terminal::poll_event()`] is generally recommended for detecting terminal resize events,
    /// you may need to call this method directly when using external I/O polling crates like `mio`.
    pub fn wait_for_resize(&mut self) -> std::io::Result<TerminalSize> {
        self.signal.read_exact(&mut [0])?;
        self.update_size()?;
        Ok(self.size)
    }

    /// Sets the cursor position to be displayed after drawing a frame.
    ///
    /// This method allows controlling where the cursor appears on the terminal after
    /// calling [`Terminal::draw()`]. Setting a position makes the cursor visible at
    /// that location, while passing `None` hides the cursor.
    ///
    /// The cursor position is only applied after drawing a frame, so it won't take
    /// effect until the next call to [`Terminal::draw()`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tuinix::{Terminal, TerminalPosition};
    ///
    /// let mut terminal = Terminal::new()?;
    ///
    /// // Show cursor at row 5, column 10
    /// terminal.set_cursor(Some(TerminalPosition::row_col(5, 10)));
    ///
    /// // Hide cursor
    /// terminal.set_cursor(None);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn set_cursor(&mut self, position: Option<TerminalPosition>) {
        self.cursor = position;
    }

    /// Draws a frame to the terminal screen.
    ///
    /// This method efficiently renders a terminal frame by
    /// only redrawing lines that differ from the previous frame.
    ///
    /// The frame is saved internally, allowing subsequent calls to only update
    /// changed portions of the screen for better performance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::fmt::Write;
    /// use tuinix::{Terminal, TerminalPosition, TerminalFrame};
    ///
    /// let mut terminal = Terminal::new()?;
    /// let mut frame: TerminalFrame = TerminalFrame::new(terminal.size());
    ///
    /// // Write some text
    /// writeln!(frame, "Hello, terminal world!")?;
    ///
    /// // Display the cursor at the beginning of the next line
    /// terminal.set_cursor(Some(TerminalPosition::row(1)));
    ///
    /// // Render the frame to the terminal
    /// terminal.draw(frame)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn draw<W>(&mut self, frame: TerminalFrame<W>) -> std::io::Result<()> {
        let frame = frame.finish();
        self.hide_cursor()?;

        let move_cursor = |output: &mut BufWriter<_>, position: TerminalPosition| {
            write!(output, "\x1b[{};{}H", position.row + 1, position.col + 1)
        };

        let resized = self.last_frame.size() != frame.size();
        let mut skipped = false;
        let mut last_style = None;
        let mut last_row = usize::MAX;
        for (position, c) in frame.chars() {
            let old = self.last_frame.get_char(position);
            if !resized && Some(c) == old {
                skipped = true;
                continue;
            }

            if skipped || last_row != position.row {
                move_cursor(&mut self.output, position)?;
            }
            if Some(c.style) != last_style {
                write!(self.output, "{}", c.style)?;
            }
            write!(self.output, "{}", c.value)?;

            last_style = Some(c.style);
            last_row = position.row;
            skipped = false;
        }

        if let Some(position) = self.cursor {
            move_cursor(&mut self.output, position)?;
            self.show_cursor()?;
        }

        self.output.flush()?;
        self.last_frame = frame;

        Ok(())
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        write!(self.output, "\x1b[?25l")
    }

    fn show_cursor(&mut self) -> std::io::Result<()> {
        write!(self.output, "\x1b[?25h")
    }

    fn update_size(&mut self) -> std::io::Result<()> {
        let mut winsize = MaybeUninit::<libc::winsize>::zeroed();
        check_libc_result(unsafe {
            libc::ioctl(self.output_fd(), libc::TIOCGWINSZ, winsize.as_mut_ptr())
        })?;

        let winsize = unsafe { winsize.assume_init() };
        self.size.rows = winsize.ws_row as usize;
        self.size.cols = winsize.ws_col as usize;

        Ok(())
    }

    fn enable_alternate_screen(&mut self) -> std::io::Result<()> {
        write!(self.output, "\x1b[?1049h")
    }

    fn disable_alternate_screen(&mut self) -> std::io::Result<()> {
        write!(self.output, "\x1b[?1049l")
    }

    fn enable_raw_mode(&mut self) -> std::io::Result<()> {
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

    fn disable_raw_mode(&mut self) -> std::io::Result<()> {
        check_libc_result(unsafe {
            libc::tcsetattr(self.input_fd(), libc::TCSAFLUSH, &self.original_termios)
        })?;
        Ok(())
    }
}

impl Drop for Terminal {
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

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal").finish()
    }
}

/// Terminal event returned by [`Terminal::poll_event()`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalEvent {
    /// Terminal resize event.
    Resize(TerminalSize),

    /// User input event.
    Input(TerminalInput),

    /// Custom file descriptor is ready for I/O.
    FdReady {
        /// The file descriptor that is ready for I/O operations.
        ///
        /// This is the raw file descriptor number that was passed to
        /// [`Terminal::poll_event()`] in either the `additional_readfds` or
        /// `additional_writefds` parameters and is now ready for reading or writing.
        fd: RawFd,

        /// `true` if ready for reading, `false` if ready for writing.
        readable: bool,
    },
}

fn check_libc_result(result: libc::c_int) -> std::io::Result<()> {
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

fn set_sigwinch_handler() -> std::io::Result<File> {
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

/// Opens a fresh, independent file description of the terminal device that `input_fd` is
/// connected to, and makes it non-blocking. The original file description is not modified.
fn open_nonblocking_input(input_fd: RawFd) -> std::io::Result<RawFd> {
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

    use super::{Terminal, open_nonblocking_input};

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

        let terminal = Terminal::new().expect("ok");

        // Creating a second terminal should fail while the first one exists
        assert!(Terminal::new().is_err());

        // After dropping the first terminal, creating a new one should succeed
        std::mem::drop(terminal);
        assert!(Terminal::new().is_ok());
    }
}
