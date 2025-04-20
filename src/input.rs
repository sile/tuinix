use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyInput),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub ctrl: bool,
    pub alt: bool,
    pub code: KeyCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Enter,
    Escape,
    Backspace,
    Tab,
    BackTab,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Char(char),
}

#[derive(Debug)]
pub struct InputReader<R> {
    inner: R,
    buf: Vec<u8>,
    buf_offset: usize,
}

impl<R: Read> InputReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: vec![0; 64],
            buf_offset: 0,
        }
    }

    pub fn inner(&self) -> &R {
        &self.inner
    }

    pub fn read_input(&mut self) -> std::io::Result<Option<Input>> {
        let read_size = self.inner.read(&mut self.buf[self.buf_offset..])?;
        if read_size == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }

        let size = self.buf_offset + read_size;
        let Some((input, consumed_size)) = self.parse_input(&self.buf[..size])? else {
            return Ok(None);
        };
        self.buf.copy_within(consumed_size..size, 0);
        self.buf_offset = 0;
        Ok(Some(input))
    }

    fn parse_input(&self, bytes: &[u8]) -> std::io::Result<Option<(Input, usize)>> {
        todo!("parse self.buf to read ANSI terminal input")
    }
}
