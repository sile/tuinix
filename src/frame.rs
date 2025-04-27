use std::collections::BTreeMap;

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

#[derive(Debug, Default, Clone)]
pub struct TerminalFrame<M = FixedCharWidthMeasurer> {
    size: TerminalSize,
    data: BTreeMap<TerminalPosition, TerminalChar>,
    tail: TerminalPosition,
    current_style: TerminalStyle,
    escape_sequence: String,
    measurer: M,
}

impl<M: MeasureCharWidth + Default> TerminalFrame<M> {
    pub fn new(size: TerminalSize) -> Self {
        Self::with_measurer(size, M::default())
    }
}

impl<M: MeasureCharWidth> TerminalFrame<M> {
    pub fn with_measurer(size: TerminalSize, measurer: M) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
            tail: TerminalPosition::ZERO,
            current_style: TerminalStyle::new(),
            escape_sequence: String::new(),
            measurer,
        }
    }

    /// Returns the size of this frame.
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub fn draw(&mut self, position: TerminalPosition, frame: &Self) {
        for (src_pos, c) in frame.chars() {
            let target_pos = position + src_pos;
            if !self.size.contains(target_pos) {
                continue;
            }

            if let Some((&prev_pos, prev_c)) = self.data.range(..target_pos).next_back() {
                let end_pos = prev_pos + TerminalPosition::col(prev_c.width);
                if target_pos < end_pos {
                    self.data.remove(&prev_pos);
                }
            }
            for i in 0..c.width {
                self.data.remove(&(target_pos + TerminalPosition::col(i)));
            }
            self.data.insert(target_pos, c);

            self.tail = self.tail.max(target_pos + TerminalPosition::col(c.width));
        }
    }

    pub(crate) fn chars(&self) -> impl '_ + Iterator<Item = (TerminalPosition, TerminalChar)> {
        let mut last_style = TerminalStyle::new();
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| TerminalPosition::row_col(row, col)))
            .map(move |pos| {
                if let Some(c) = self.data.get(&pos).copied() {
                    last_style = c.style;
                    (pos, c)
                } else {
                    if pos >= self.tail {
                        last_style = self.current_style;
                    }
                    let c = TerminalChar {
                        style: last_style,
                        width: 1,
                        value: ' ',
                    };
                    (pos, c)
                }
            })
    }
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
                continue;
            }

            let width = self.measurer.measure_char_width(c);
            if width == 9 {
                continue;
            }

            if self.tail.col + width < self.size.cols {
                self.data.insert(
                    self.tail,
                    TerminalChar {
                        style: self.current_style,
                        width,
                        value: c,
                    },
                );
            }
            self.tail.col += width;
        }

        Ok(())
    }
}

pub trait MeasureCharWidth {
    fn measure_char_width(&self, c: char) -> usize;
}

#[derive(Debug, Default, Clone)]
pub struct FixedCharWidthMeasurer;

impl MeasureCharWidth for FixedCharWidthMeasurer {
    fn measure_char_width(&self, c: char) -> usize {
        if c.is_control() { 0 } else { 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalChar {
    pub style: TerminalStyle,
    pub width: usize,
    pub value: char,
}
