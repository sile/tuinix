use std::ops::{Add, AddAssign, Sub, SubAssign};

/// Dimensions of a terminal.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalSize {
    /// Number of rows (height) in the terminal.
    pub rows: usize,

    /// Number of columns (width) in the terminal.
    pub cols: usize,
}

impl TerminalSize {
    /// Returns `true` if the terminal has zero rows or zero columns.
    pub const fn is_empty(self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    pub const fn contains(self, position: TerminalPosition) -> bool {
        position.row < self.rows && position.col < self.cols
    }
}

/// Position within a terminal.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalPosition {
    /// Row coordinate (vertical position, 0-indexed from the top).
    pub row: usize,

    /// Column coordinate (horizontal position, 0-indexed from the left).
    pub col: usize,
}

impl TerminalPosition {
    /// Origin position (0,0).
    pub const ZERO: Self = Self::row_col(0, 0);

    /// Makes a new position with the specified row and column coordinates.
    pub const fn row_col(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    /// Makes a new position at the beginning of the specified row.
    ///
    /// This is a convenience constructor that sets the column to 0.
    pub const fn row(row: usize) -> Self {
        Self::row_col(row, 0)
    }
}

impl Add for TerminalPosition {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self {
            row: self.row + other.row,
            col: self.col + other.col,
        }
    }
}

impl AddAssign for TerminalPosition {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sub for TerminalPosition {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self {
            row: self.row.saturating_sub(other.row),
            col: self.col.saturating_sub(other.col),
        }
    }
}

impl SubAssign for TerminalPosition {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}
