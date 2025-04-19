use std::{
    io::{IsTerminal, Stdin, Stdout, Write},
    mem::MaybeUninit,
    os::fd::AsRawFd,
};

pub struct Terminal {
    stdin: Stdin,
    stdout: Stdout,
    original: libc::termios,
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

        let mut this = Self {
            stdin,
            stdout,
            original: unsafe { termios.assume_init() },
        };
        this.enable_raw_mode()?;
        this.enable_alternate_screen()?;
        this.stdout.flush()?;

        Ok(this)
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
