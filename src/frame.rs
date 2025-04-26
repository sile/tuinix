use crate::{TerminalPosition, TerminalSize};

#[derive(Debug, Default, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    data: String,
    tail: TerminalPosition,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            data: String::new(),
            tail: TerminalPosition::ZERO,
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub(crate) fn lines(&self) -> impl '_ + Iterator<Item = &str> {
        self.data
            .lines()
            .chain(std::iter::repeat(""))
            .take(self.size.rows)
    }

    // TODO: merge or draw_frame
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.tail.row >= self.size.rows {
            return Ok(());
        }

        for c in s.chars() {
            if c == '\n' {
                self.tail.row += 1;
                self.tail.col = 0;
                if self.tail.row >= self.size.rows {
                    return Ok(());
                }
            }

            // TODO: consider cols

            self.data.push(c);
        }

        Ok(())
    }
}
