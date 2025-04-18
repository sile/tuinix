use std::{
    io::{IsTerminal, Stdin, Stdout, Write},
    mem::MaybeUninit,
    os::fd::AsRawFd,
};

// TODO: #[derive(Debug)]
pub struct Terminal {
    stdin: Stdin,
    stdout: Stdout,
    original_termios: libc::termios,
}

impl Terminal {
    pub fn new() -> std::io::Result<Self> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
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

        // Enable alternate screen
        write!(stdout, "\x1b[?1049h")?;
        stdout.flush()?;

        // TODO: non blocking

        Ok(Self {
            stdin,
            stdout,
            original_termios: unsafe { termios.assume_init() },
        })
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Disable alternate screen
        let _ = write!(self.stdout, "\x1b[?1049l");
        let _ = self.stdout.flush();
        unsafe {
            libc::tcsetattr(
                self.stdin.as_raw_fd(),
                libc::TCSANOW,
                &self.original_termios,
            );
        }
    }
}
