use std::collections::BTreeMap;

use unicode_width::UnicodeWidthChar;

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

#[derive(Debug, Default, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    cursor: TerminalPosition,
    show_cursor: bool,
    chars: BTreeMap<TerminalPosition, TerminalChar>,
    current_style: TerminalStyle,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            cursor: TerminalPosition::default(),
            show_cursor: false,
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

    pub fn set_show_cursor(&mut self, b: bool) {
        self.show_cursor = b;
    }

    // TODO: rename
    pub fn show_cursor(&self) -> bool {
        self.show_cursor
    }

    pub(crate) fn get_line(
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
        if self.cursor.row >= self.size.rows {
            return Ok(());
        }

        while let Some(c) = s.chars().next() {
            match c {
                '\n' => {
                    self.cursor.row += 1;
                    self.cursor.col = 0;
                    if self.cursor.row >= self.size.rows {
                        break;
                    }
                    // TODO(?): Add TerminalStyle::RESET
                    s = &s[1..];
                }
                '\x1b' => {
                    s = self.current_style.update(&s[1..]);
                }
                _ => {
                    self.push_char(c);
                    s = &s[c.len_utf8()..];
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalChar {
    pub value: char,
    pub width: usize,
    pub style: TerminalStyle,
}
