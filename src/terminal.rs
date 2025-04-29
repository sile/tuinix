use std::{
    fs::File,
    io::{BufWriter, Error, ErrorKind, IsTerminal, Read, Stdin, Stdout, Write},
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
///     let mut frame = TerminalFrame::new(size);
///     // Add content to frame...
///     terminal.draw(frame)?;
///
///     // Wait for events with timeout
///     let timeout = Duration::from_millis(100);
///     if let Some(event) = terminal.poll_event(Some(timeout))? {
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
/// use tuinix::{Terminal, TerminalFrame, set_nonblocking, try_nonblocking, try_uninterrupted};
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
///     let stdin_fd = terminal.input_fd();
///     let signal_fd = terminal.signal_fd();
///     set_nonblocking(stdin_fd)?;
///     set_nonblocking(signal_fd)?;
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
    input: InputReader<Stdin>,
    output: BufWriter<Stdout>,
    signal: File,
    original_termios: libc::termios,
    size: TerminalSize,
    last_frame: TerminalFrame,
    cursor: Option<TerminalPosition>,
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
            return Err(Error::new(
                ErrorKind::Other,
                "Terminal instance already exists",
            ));
        }

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        if !stdin.is_terminal() {
            return Err(Error::new(ErrorKind::Other, "STDIN is not a terminal"));
        }
        if !stdout.is_terminal() {
            return Err(Error::new(ErrorKind::Other, "STDOUT is not a terminal"));
        }

        let mut termios = MaybeUninit::<libc::termios>::zeroed();
        check_libc_result(unsafe { libc::tcgetattr(stdin.as_raw_fd(), termios.as_mut_ptr()) })?;
        let original_termios = unsafe { termios.assume_init() };

        let mut this = Self {
            input: InputReader::new(stdin),
            output: BufWriter::new(stdout),
            signal: set_sigwinch_handler()?,
            original_termios,
            size: TerminalSize::default(),
            last_frame: TerminalFrame::default(),
            cursor: None,
        };
        this.update_size()?;
        this.enable_raw_mode()?;
        this.enable_alternate_screen()?;
        this.hide_cursor()?;
        this.output.flush()?;

        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Disable alternate screen and raw mode to show the panic message
            let mut stdout = std::io::stdout();
            let stdin = std::io::stdin();
            unsafe {
                libc::tcsetattr(stdin.as_raw_fd(), libc::TCSAFLUSH, &original_termios);
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
    pub fn input_fd(&self) -> RawFd {
        self.input.inner().as_raw_fd()
    }

    /// Returns the file descriptor of the terminal output.
    pub fn output_fd(&self) -> RawFd {
        self.output.get_ref().as_raw_fd()
    }

    /// Returns the file descriptor that receives terminal resize signal notifications.
    pub fn signal_fd(&self) -> RawFd {
        self.signal.as_raw_fd()
    }

    /// Waits for and returns the next terminal event.
    ///
    /// This method efficiently waits for either input events or terminal resize events
    /// using [`libc::select()`].
    ///
    /// If you want to use I/O polling mechanisms other than [`libc::select()`],
    /// please use the following methods directly:
    /// - [`Terminal::input_fd()`] and [`Terminal::read_input()`] for input events
    /// - [`Terminal::signal_fd()`] and [`Terminal::wait_for_resize()`] for resize events
    ///
    /// # Returns
    ///
    /// - `Ok(Some(TerminalEvent))` if either an input or resize event was received
    /// - `Ok(None)` if the timeout expired without any event
    /// - `Err(e)` if an I/O error occurred
    pub fn poll_event(
        &mut self,
        timeout: Option<Duration>,
    ) -> std::io::Result<Option<TerminalEvent>> {
        if let Some(input) = self.input.read_input_from_buf()? {
            return Ok(Some(TerminalEvent::Input(input)));
        }

        let start_time = Instant::now();
        loop {
            unsafe {
                let mut readfds = MaybeUninit::<libc::fd_set>::zeroed();
                libc::FD_ZERO(readfds.as_mut_ptr());
                libc::FD_SET(self.input_fd(), readfds.as_mut_ptr());
                libc::FD_SET(self.signal_fd(), readfds.as_mut_ptr());
                let mut readfds = readfds.assume_init();

                let maxfd = self.input_fd().max(self.signal.as_raw_fd());

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
                    std::ptr::null_mut(),
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

                if libc::FD_ISSET(self.input_fd(), &readfds) {
                    if let Some(input) = self.read_input()? {
                        return Ok(Some(TerminalEvent::Input(input)));
                    }
                }
                if libc::FD_ISSET(self.signal_fd(), &readfds) {
                    return self.wait_for_resize().map(TerminalEvent::Resize).map(Some);
                }
            }
        }
    }

    /// Reads and processes the next input event from the terminal.
    ///
    /// This method attempts to read raw bytes from stdin and parse them into a
    /// structured [`TerminalInput`] event.
    ///
    /// By default, this method blocks until input is available. To use it in non-blocking
    /// mode, first call [`set_nonblocking()`](crate::set_nonblocking) on [`Terminal::input_fd()`].
    ///
    /// While [`Terminal::poll_event()`] is generally recommended for receiving terminal input events,
    /// you may need to call this method directly when using external I/O polling crates like `mio`.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(input))` if an input event was successfully read and parsed
    /// - `Ok(None)` if not enough bytes were available to form a complete input event
    /// - `Err(e)` if an I/O error occurred while reading from stdin
    ///
    /// # Errors
    ///
    /// This method returns an error if reading from stdin fails or encounters EOF.
    pub fn read_input(&mut self) -> std::io::Result<Option<TerminalInput>> {
        self.input.read_input()
    }

    /// Waits for a terminal resize event to occur and returns the new terminal size.
    ///
    /// By default, this method blocks until input is available. To use it in non-blocking
    /// mode, first call [`set_nonblocking()`](crate::set_nonblocking) on [`Terminal::signal_fd()`].
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
    /// let mut frame = TerminalFrame::new(terminal.size());
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

        if self.last_frame.size() != frame.size() {
            write!(self.output, "\x1b[2J")?; // Clear screen
            self.last_frame = TerminalFrame::new(frame.size());
        }

        let move_cursor = |output: &mut BufWriter<_>, position: TerminalPosition| {
            write!(output, "\x1b[{};{}H", position.row + 1, position.col + 1)
        };

        let mut skipped = false;
        let mut last_style = None;
        let mut last_row = usize::MAX;
        for (new, old) in frame.chars().zip(self.last_frame.chars()) {
            if new == old {
                skipped = true;
                continue;
            }
            let (position, Some(c)) = new else {
                continue;
            };

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

/// Terminal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalEvent {
    /// Terminal resize event.
    Resize(TerminalSize),

    /// User input event.
    Input(TerminalInput),
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

        sigaction.sa_sigaction = handle_sigwinch as libc::sighandler_t;
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

#[cfg(test)]
mod tests {
    use std::io::IsTerminal;

    use super::Terminal;

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
