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

    pub(crate) fn get_line(&self, row: usize) -> (TerminalStyle, &str) {
        // let start = TerminalPosition { row, col: 0 };
        // let end = TerminalPosition {
        //     row: row + 1,
        //     col: 0,
        // };
        // self.chars.range(start..end).map(|(p, c)| (*p, *c))
        todo!()
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
    fn write_str(&mut self, mut s: &str) -> std::fmt::Result {
        // if self.cursor.row >= self.size.rows {
        //     return Ok(());
        // }

        // while let Some(c) = s.chars().next() {
        //     match c {
        //         '\n' => {
        //             self.cursor.row += 1;
        //             self.cursor.col = 0;
        //             if self.cursor.row >= self.size.rows {
        //                 break;
        //             }
        //             // TODO(?): Add TerminalStyle::RESET
        //             s = &s[1..];
        //         }
        //         '\x1b' => {
        //             //s = self.current_style.update(&s[1..]);
        //             todo!()
        //         }
        //         _ => {
        //             self.push_char(c);
        //             s = &s[c.len_utf8()..];
        //         }
        //     }
        // }
        // Ok(())
        todo!()
    }
}
