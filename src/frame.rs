use crate::terminal::TerminalSize;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    lines: Vec<TerminalLine>,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            lines: vec![TerminalLine::new(size.cols); size.rows],
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }
}

#[derive(Debug, Clone)]
pub struct TerminalLine {}

impl TerminalLine {
    pub fn new(_cols: usize) -> Self {
        Self {}
    }
}
