use std::collections::BTreeMap;

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

#[derive(Debug, Default, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    data: BTreeMap<TerminalPosition, TerminalChar>,
    tail: TerminalPosition,
    current_style: TerminalStyle,
    escape_sequence: String,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
            tail: TerminalPosition::ZERO,
            current_style: TerminalStyle::new(),
            escape_sequence: String::new(),
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub(crate) fn lines(&self) -> impl '_ + Iterator<Item = &str> {
        // self.data
        //     .lines()
        //     .chain(std::iter::repeat(""))
        //     .take(self.size.rows)
        std::iter::empty()
    }

    // TODO: merge or draw_frame
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.tail.row >= self.size.rows {
            return Ok(());
        }

        for c in s.chars() {
            if !self.escape_sequence.is_empty() {
                self.escape_sequence.push(c);
                if c.is_ascii_alphabetic() {
                    self.current_style = self
                        .escape_sequence
                        .parse()
                        .expect("escape sequence should be generated via `TerminalStyle`");
                    self.escape_sequence.clear();
                }
                continue;
            } else if c == '\x1b' {
                self.escape_sequence.push(c);
                continue;
            } else if c == '\n' {
                self.tail.row += 1;
                self.tail.col = 0;
                if self.tail.row >= self.size.rows {
                    return Ok(());
                }
            }

            if c.is_control() {
                continue;
            }

            if self.tail.col < self.size.cols {
                self.data.insert(
                    self.tail,
                    TerminalChar {
                        style: self.current_style,
                        value: c,
                    },
                );
            }

            // TODO: consider char width
            self.tail.col += 1;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct TerminalChar {
    style: TerminalStyle,
    value: char,
}
