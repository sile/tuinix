use crate::{TerminalPosition, TerminalSize, TerminalStyle};

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

    pub(crate) fn lines(&self) -> impl '_ + Iterator<Item = (TerminalStyle, &str)> {
        // let start = TerminalPosition { row, col: 0 };
        // let end = TerminalPosition {
        //     row: row + 1,
        //     col: 0,
        // };
        // self.chars.range(start..end).map(|(p, c)| (*p, *c))
        std::iter::empty()
    }

    // TODO: merge or draw_frame

    // fn push_char(&mut self, mut c: char) {
    //     if self.cursor.col >= self.size.cols {
    //         return;
    //     }

    //     let width = if let Some(width) = c.width() {
    //         width
    //     } else {
    //         // control char - use replacement character (tofu)
    //         c = '�';
    //         1
    //     };

    //     let c = TerminalChar {
    //         value: c,
    //         width,
    //         style: self.current_style,
    //     };
    //     self.chars.insert(self.cursor, c);
    //     self.cursor.col += width;
    // }
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.tail.row >= self.size.rows {
            return Ok(());
        }

        for c in s.chars() {
            if c == '\n' {
                // TODO: padding if need

                self.tail.row += 1;
                self.tail.col = 0;
                if self.tail.row >= self.size.rows {
                    return Ok(());
                }
            }

            self.data.push(c);
        }

        Ok(())
    }
}
