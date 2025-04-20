use std::{
    fs::File,
    io::{IsTerminal, Read, Stdin, Stdout, Write},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, RawFd},
    time::{Duration, Instant},
};

use crate::frame::{TerminalFrame, TerminalPosition, TerminalStyle};

static mut SIGWINCH_PIPE_FD: RawFd = 0;

unsafe extern "C" fn handle_sigwinch(_: libc::c_int) {
    unsafe {
        let _ = libc::write(SIGWINCH_PIPE_FD, [0].as_ptr().cast(), 1);
    }
}

pub struct Terminal {
    stdin: Stdin,
    stdout: Stdout,
    sigwinch: File,
    original: libc::termios,
    size: TerminalSize,
    cursor: Option<TerminalPosition>,
    last_frame: TerminalFrame,
}

impl Terminal {
    pub fn new() -> std::io::Result<Self> {
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
        if unsafe { libc::tcgetattr(stdin.as_raw_fd(), termios.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }

        // TODO: non blocking

        // TODO: duplicate check
        let mut pipefd = [0 as RawFd; 2];
        if unsafe { libc::pipe(pipefd.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe {
            SIGWINCH_PIPE_FD = pipefd[1];

            let mut sigaction = MaybeUninit::<libc::sigaction>::zeroed().assume_init();
            sigaction.sa_sigaction = handle_sigwinch as libc::sighandler_t;
            sigaction.sa_flags = 0;

            if libc::sigemptyset(&mut sigaction.sa_mask) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            if libc::sigaction(libc::SIGWINCH, &sigaction, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }

        let mut this = Self {
            stdin,
            stdout,
            sigwinch: unsafe { File::from_raw_fd(pipefd[0]) },
            original: unsafe { termios.assume_init() },
            size: TerminalSize::default(),
            cursor: Some(TerminalPosition::ZERO),
            last_frame: TerminalFrame::default(),
        };
        this.enable_raw_mode()?;
        this.enable_alternate_screen()?;
        this.stdout.flush()?;
        this.update_size()?;
        this.set_cursor(None)?;

        Ok(this)
    }

    pub fn poll_event(&mut self, timeout: Option<Duration>) -> std::io::Result<Option<Event>> {
        let start_time = Instant::now();
        loop {
            unsafe {
                let mut readfds = MaybeUninit::<libc::fd_set>::zeroed();
                libc::FD_ZERO(readfds.as_mut_ptr());
                libc::FD_SET(self.stdin.as_raw_fd(), readfds.as_mut_ptr());
                libc::FD_SET(self.sigwinch.as_raw_fd(), readfds.as_mut_ptr());
                let mut readfds = readfds.assume_init();

                let maxfd = self.stdin.as_raw_fd().max(self.sigwinch.as_raw_fd());

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
                    return Err(std::io::Error::last_os_error());
                } else if ret == 0 {
                    // Timeout
                    return Ok(None);
                }

                if libc::FD_ISSET(self.stdin.as_raw_fd(), &readfds) {
                    if let Some(input) = self.read_input()? {
                        return Ok(Some(Event::Input(input)));
                    }
                } else {
                    assert!(libc::FD_ISSET(self.sigwinch.as_raw_fd(), &readfds));
                    return self.read_size().map(Event::TerminalSize).map(Some);
                }
            }
        }
    }

    pub fn read_size(&mut self) -> std::io::Result<TerminalSize> {
        self.sigwinch.read_exact(&mut [0])?;
        self.update_size()?;
        Ok(self.size)
    }

    pub fn read_input(&mut self) -> std::io::Result<Option<()>> {
        todo!()
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
            (Some(_), None) => write!(self.stdout, "\x1b[?25l")?,
            (None, Some(_)) => write!(self.stdout, "\x1b[?25h")?,
            _ => {}
        }
        if let Some(position) = position {
            write!(
                self.stdout,
                "\x1b[{};{}H",
                position.row + 1,
                position.col + 1
            )?;
        }
        self.cursor = position;
        self.stdout.flush()?;
        Ok(())
    }

    fn update_size(&mut self) -> std::io::Result<()> {
        let mut winsize = MaybeUninit::<libc::winsize>::zeroed();
        if unsafe {
            libc::ioctl(
                self.stdout.as_raw_fd(),
                libc::TIOCGWINSZ,
                winsize.as_mut_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }

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
                    self.stdout,
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
        write!(self.stdout, "\x1b[?1049h")
    }

    fn disable_alternate_screen(&mut self) -> std::io::Result<()> {
        write!(self.stdout, "\x1b[?1049l")
    }

    fn enable_raw_mode(&mut self) -> std::io::Result<()> {
        let mut raw = self.original;

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

        if unsafe { libc::tcsetattr(self.stdin.as_raw_fd(), libc::TCSAFLUSH, &raw) } != 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn disable_raw_mode(&mut self) -> std::io::Result<()> {
        if unsafe { libc::tcsetattr(self.stdin.as_raw_fd(), libc::TCSAFLUSH, &self.original) } != 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.disable_raw_mode();
        let _ = self.disable_alternate_screen();
        let _ = self.stdout.flush();
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
    TerminalSize(TerminalSize),
    Input(()),
}
