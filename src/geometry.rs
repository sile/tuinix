#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
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

    pub const fn row(row: usize) -> Self {
        Self::row_col(row, 0)
    }
}
