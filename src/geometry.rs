use std::ops::{Add, AddAssign, Sub, SubAssign};

/// Dimensions of a [`Terminal`](crate::Terminal) or [`TerminalFrame`](crate::TerminalFrame).
///
/// This structure stores the number of rows (height) and columns (width) that define
/// the size of a terminal display area.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalSize {
    /// Number of rows (height) in the terminal.
    pub rows: usize,

    /// Number of columns (width) in the terminal.
    pub cols: usize,
}

impl TerminalSize {
    /// A terminal size with zero rows and zero columns.
    pub const EMPTY: Self = Self { rows: 0, cols: 0 };

    /// Creates a new terminal size with the given number of rows and columns.
    pub const fn rows_cols(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    /// Returns the total area (number of cells) represented by this size.
    pub const fn area(self) -> usize {
        self.rows * self.cols
    }

    /// Returns `true` if the terminal has zero rows or zero columns.
    pub const fn is_empty(self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    /// Returns `true` if the given position falls within the boundaries of this terminal size.
    pub const fn contains(self, position: TerminalPosition) -> bool {
        position.row < self.rows && position.col < self.cols
    }

    /// Converts this size into a region starting at the origin.
    pub const fn to_region(self) -> TerminalRegion {
        TerminalRegion {
            position: TerminalPosition::ZERO,
            size: self,
        }
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

    /// Makes a new position with the specified column at the first row.
    pub const fn col(col: usize) -> Self {
        Self::row_col(0, col)
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

/// A rectangular region within a terminal, defined by a position and size.
///
/// This structure represents a bounded area within a terminal, useful for
/// creating sub-regions or windows within the terminal display.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalRegion {
    /// The top-left position of the region.
    pub position: TerminalPosition,

    /// The size (dimensions) of the region.
    pub size: TerminalSize,
}

impl TerminalRegion {
    /// Returns `true` if the region has zero area (either zero rows or zero columns).
    pub const fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// Returns `true` if the given position falls within this region.
    pub const fn contains(self, position: TerminalPosition) -> bool {
        position.row >= self.position.row
            && position.col >= self.position.col
            && position.row < self.position.row + self.size.rows
            && position.col < self.position.col + self.size.cols
    }

    /// Returns the top-left position of the region.
    pub const fn top_left(self) -> TerminalPosition {
        self.position
    }

    /// Returns the top-right position of the region.
    pub const fn top_right(self) -> TerminalPosition {
        TerminalPosition::row_col(
            self.position.row,
            self.position.col + self.size.cols.saturating_sub(1),
        )
    }

    /// Returns the bottom-left position of the region.
    pub const fn bottom_left(self) -> TerminalPosition {
        TerminalPosition::row_col(
            self.position.row + self.size.rows.saturating_sub(1),
            self.position.col,
        )
    }

    /// Returns the bottom-right position of the region.
    pub const fn bottom_right(self) -> TerminalPosition {
        TerminalPosition::row_col(
            self.position.row + self.size.rows.saturating_sub(1),
            self.position.col + self.size.cols.saturating_sub(1),
        )
    }

    /// Returns a new region containing only the top N rows.
    pub const fn take_top(mut self, rows: usize) -> Self {
        if rows < self.size.rows {
            self.size.rows = rows;
        }
        self
    }

    /// Returns a new region containing only the bottom N rows.
    pub const fn take_bottom(mut self, rows: usize) -> Self {
        if rows < self.size.rows {
            let offset = self.size.rows - rows;
            self.position.row += offset;
            self.size.rows = rows;
        }
        self
    }

    /// Returns a new region containing only the leftmost N columns.
    pub const fn take_left(mut self, cols: usize) -> Self {
        if cols < self.size.cols {
            self.size.cols = cols;
        }
        self
    }

    /// Returns a new region containing only the rightmost N columns.
    pub const fn take_right(mut self, cols: usize) -> Self {
        if cols < self.size.cols {
            let offset = self.size.cols - cols;
            self.position.col += offset;
            self.size.cols = cols;
        }
        self
    }

    /// Returns a new region with the top N rows removed.
    pub const fn drop_top(mut self, rows: usize) -> Self {
        if rows < self.size.rows {
            self.position.row += rows;
            self.size.rows -= rows;
        } else {
            self.size.rows = 0;
        }
        self
    }

    /// Returns a new region with the bottom N rows removed.
    pub const fn drop_bottom(mut self, rows: usize) -> Self {
        if rows < self.size.rows {
            self.size.rows -= rows;
        } else {
            self.size.rows = 0;
        }
        self
    }

    /// Returns a new region with the leftmost N columns removed.
    pub const fn drop_left(mut self, cols: usize) -> Self {
        if cols < self.size.cols {
            self.position.col += cols;
            self.size.cols -= cols;
        } else {
            self.size.cols = 0;
        }
        self
    }

    /// Returns a new region with the rightmost N columns removed.
    pub const fn drop_right(mut self, cols: usize) -> Self {
        if cols < self.size.cols {
            self.size.cols -= cols;
        } else {
            self.size.cols = 0;
        }
        self
    }
}
