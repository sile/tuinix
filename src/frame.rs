use crate::terminal::TerminalSize;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self { size }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalChar {
    pub value: char,
    pub style: TerminalStyle,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub dim: bool,
    pub strikethrough: bool,
    pub fg_color: Option<Rgb>,
    pub bg_color: Option<Rgb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
