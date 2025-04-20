use std::collections::BTreeMap;

use unicode_width::UnicodeWidthChar;

use crate::terminal::TerminalSize;

#[derive(Debug, Default, Clone)]
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

    pub fn set_cursor(&mut self, position: TerminalPosition) {
        self.current_style = TerminalStyle::default();
        self.cursor = position;
    }

    pub fn cursor(&self) -> TerminalPosition {
        self.cursor
    }

    pub fn get_line(
        &self,
        row: usize,
    ) -> impl '_ + Iterator<Item = (TerminalPosition, TerminalChar)> {
        let start = TerminalPosition { row, col: 0 };
        let end = TerminalPosition {
            row: row + 1,
            col: 0,
        };
        self.chars.range(start..end).map(|(p, c)| (*p, *c))
    }

    // TODO: merge or draw_frame

    fn push_char(&mut self, mut c: char) {
        if self.cursor.col >= self.size.cols {
            return;
        }

        let width = if let Some(width) = c.width() {
            width
        } else {
            // control char - use replacement character (tofu)
            c = '�';
            1
        };

        let c = TerminalChar {
            value: c,
            width,
            style: self.current_style,
        };
        self.chars.insert(self.cursor, c);
        self.cursor.col += width;
    }
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, mut s: &str) -> std::fmt::Result {
        while self.cursor.row < self.size.rows {
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

impl TerminalPosition {
    pub const ZERO: Self = Self::row_col(0, 0);

    pub const fn row_col(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalChar {
    pub value: char,
    pub width: usize,
    pub style: TerminalStyle,
}

// TODO: attrs?
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
        let (s, remaining) = s
            .split_once('m')
            .expect("Expected 'm' terminator for ANSI escape sequence");
        match s {
            "0" => *self = TerminalStyle::default(),
            "1" => self.bold = true,
            "2" => self.dim = true,
            "3" => self.italic = true,
            "4" => self.underline = true,
            "5" => self.blink = true,
            "7" => self.reverse = true,
            "9" => self.strikethrough = true,
            _ => {
                let (fg, s) = if let Some(s) = s.strip_prefix("38;2;") {
                    (true, s)
                } else if let Some(s) = s.strip_prefix("48;2;") {
                    (false, s)
                } else {
                    panic!(
                        "Unsupported ANSI color format - expected 38;2; (foreground) or 48;2; (background) TrueColor sequence"
                    );
                };

                let (r, s) = s.split_once(';').expect(
                    "Invalid RGB format in ANSI color - expected ';' separator after red component",
                );
                let (g, b) = s.split_once(';').expect("Invalid RGB format in ANSI color - expected ';' separator after green component");
                let r = r
                    .parse()
                    .expect("Invalid red color value in ANSI RGB sequence - expected u8 value");
                let g = g
                    .parse()
                    .expect("Invalid green color value in ANSI RGB sequence - expected u8 value");
                let b = b
                    .parse()
                    .expect("Invalid blue color value in ANSI RGB sequence - expected u8 value");
                if fg {
                    self.fg_color = Some(Rgb { r, g, b });
                } else {
                    self.bg_color = Some(Rgb { r, g, b });
                }
            }
        }

        remaining
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
