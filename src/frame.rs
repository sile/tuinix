use std::collections::BTreeMap;

use unicode_width::UnicodeWidthChar;

use crate::terminal::TerminalSize;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    cursor: TerminalPosition,
    chars: BTreeMap<TerminalPosition, TerminalChar>,
    current_style: TerminalStyle,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            cursor: TerminalPosition::default(),
            chars: BTreeMap::new(),
            current_style: TerminalStyle::default(),
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    fn push_char(&mut self, c: char) {
        let Some(width) = c.width() else {
            // control char
            return;
        };

        let c = TerminalChar {
            value: c,
            style: self.current_style,
        };
        self.chars.insert(self.cursor, c);
        self.cursor.col += width;
    }
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, mut s: &str) -> std::fmt::Result {
        loop {
            for (i, c) in s.char_indices() {
                match c {
                    '\n' => {
                        self.cursor.row += 1;
                        self.cursor.col = 0;
                    }
                    '\x1b' => {
                        s = self.current_style.update(&s[i + 1..]);
                        continue;
                    }
                    _ => {
                        self.push_char(c);
                    }
                }
            }
            break;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalPosition {
    pub row: usize,
    pub col: usize,
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

impl TerminalStyle {
    fn update<'a>(&mut self, s: &'a str) -> &'a str {
        let s = s
            .strip_prefix('[')
            .expect("Expected '[' after escape character '\\x1b' for valid ANSI escape sequence");
        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
