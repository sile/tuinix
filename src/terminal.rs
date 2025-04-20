use std::{
    fs::File,
    io::{BufWriter, IsTerminal, Read, Stdin, Stdout, Write},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, RawFd},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{
    frame::{TerminalFrame, TerminalPosition, TerminalStyle},
    input::{Input, InputReader},
};

static TERMINAL_EXISTS: AtomicBool = AtomicBool::new(false);

static mut SIGWINCH_PIPE_FD: RawFd = 0;

unsafe extern "C" fn handle_sigwinch(_: libc::c_int) {
    unsafe {
        let _ = libc::write(SIGWINCH_PIPE_FD, [0].as_ptr().cast(), 1);
    }
}

// TODO: TerminalOptions{ non_blocking_stdin, ..}

pub struct Terminal {
    input: InputReader<Stdin>,
    output: BufWriter<Stdout>,
    signal: File,
    original_termios: libc::termios,
    size: TerminalSize,
    cursor: Option<TerminalPosition>,
    last_frame: TerminalFrame,
}

impl Terminal {
    pub fn new() -> std::io::Result<Self> {
        if TERMINAL_EXISTS.swap(true, Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Terminal instance already exists",
            ));
        }

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        if !stdin.is_terminal() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "STDIN is not a terminal",
            ));
        }
        if !stdout.is_terminal() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "STDOUT is not a terminal",
            ));
        }

        let mut termios = MaybeUninit::<libc::termios>::zeroed();
        check_libc_result(unsafe { libc::tcgetattr(stdin.as_raw_fd(), termios.as_mut_ptr()) })?;

        // TODO: non blocking

        // TODO: duplicate check
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
        }

        let original_termios = unsafe { termios.assume_init() };
        let mut this = Self {
            input: InputReader::new(stdin),
            output: BufWriter::new(stdout),
            signal: unsafe { File::from_raw_fd(pipefd[0]) },
            original_termios,
            size: TerminalSize::default(),
            cursor: Some(TerminalPosition::ZERO),
            last_frame: TerminalFrame::default(),
        };
        this.enable_raw_mode()?;
        this.enable_alternate_screen()?;
        this.output.flush()?;
        this.update_size()?;
        this.set_cursor(None)?;

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

    pub fn input_fd(&self) -> RawFd {
        self.input.inner().as_raw_fd()
    }

    pub fn output_fd(&self) -> RawFd {
        self.output.get_ref().as_raw_fd()
    }

    pub fn signal_fd(&self) -> RawFd {
        self.signal.as_raw_fd()
    }

    pub fn poll_event(&mut self, timeout: Option<Duration>) -> std::io::Result<Option<Event>> {
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
                    let e = std::io::Error::last_os_error();
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                } else if ret == 0 {
                    // Timeout
                    return Ok(None);
                }

                if libc::FD_ISSET(self.input_fd(), &readfds) {
                    if let Some(input) = self.input.read_input()? {
                        return Ok(Some(Event::Input(input)));
                    }
                }
                if libc::FD_ISSET(self.signal_fd(), &readfds) {
                    return self.read_size().map(Event::TerminalSize).map(Some);
                }
            }
        }
    }

    pub fn read_size(&mut self) -> std::io::Result<TerminalSize> {
        self.signal.read_exact(&mut [0])?;
        self.update_size()?;
        Ok(self.size)
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub fn cursor(&self) -> Option<TerminalPosition> {
        self.cursor
    }

    // TODO: Move to TerminalFrame? or in draw()
    pub fn set_cursor(&mut self, position: Option<TerminalPosition>) -> std::io::Result<()> {
        match (self.cursor, position) {
            (Some(_), None) => write!(self.output, "\x1b[?25l")?,
            (None, Some(_)) => write!(self.output, "\x1b[?25h")?,
            _ => {}
        }
        if let Some(position) = position {
            write!(
                self.output,
                "\x1b[{};{}H",
                position.row + 1,
                position.col + 1
            )?;
        }
        self.cursor = position;
        self.output.flush()?;
        Ok(())
    }

    fn update_size(&mut self) -> std::io::Result<()> {
        let mut winsize = MaybeUninit::<libc::winsize>::zeroed();
        check_libc_result(unsafe {
            libc::ioctl(self.output_fd(), libc::TIOCGWINSZ, winsize.as_mut_ptr())
        })?;

        let winsize = unsafe { winsize.assume_init() };
        self.size.rows = winsize.ws_row as usize;
        self.size.cols = winsize.ws_col as usize;

        // TODO: clear if the size was changed.

        Ok(())
    }

    pub fn draw(&mut self, frame: TerminalFrame) -> std::io::Result<()> {
        // TODO: save and restore cursor position if visible
        for row in 0..self.size.rows {
            if frame.get_line(row).eq(self.last_frame.get_line(row)) {
                continue;
            }

            // TODO: clear line
            // TODO: move cursor
            let mut last_style = TerminalStyle::default();
            let mut next_col = 0;
            for (TerminalPosition { col, .. }, c) in frame.get_line(row) {
                if last_style != c.style {
                    // TODO: clear style
                    last_style = c.style;
                    // TODO: write style
                }

                write!(
                    self.output,
                    "{:spaces$}{}",
                    "",
                    c.value,
                    spaces = col - next_col
                )?;
                next_col = col + c.width;
            }

            // TODO: clear style
        }

        self.last_frame = frame;
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
        let _ = self.output.flush();
        TERMINAL_EXISTS.store(false, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal").finish()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone)]
pub enum Event {
    TerminalSize(TerminalSize), // TODO: Signal
    Input(Input),
}

fn check_libc_result(result: libc::c_int) -> std::io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
