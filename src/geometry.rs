/// The number of rows and columns of a terminal display area.
///
/// This describes the extent of a [`Frame`](crate::Frame) or a
/// [`Region`], and it is also how [`TerminalDriver::size()`](crate::TerminalDriver::size)
/// reports the physical terminal size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Size {
    /// Number of rows (height).
    pub rows: usize,

    /// Number of columns (width).
    pub cols: usize,
}

impl Size {
    /// Returns `true` if this size has zero rows or zero columns.
    pub const fn is_empty(self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    /// Returns a region that starts at the origin and has this size.
    pub const fn to_region(self) -> Region {
        Region {
            position: Position::ORIGIN,
            size: self,
        }
    }
}

/// Position within a terminal.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Row coordinate (vertical position, 0-indexed from the top).
    pub row: usize,

    /// Column coordinate (horizontal position, 0-indexed from the left).
    pub col: usize,
}

impl Position {
    /// Origin position (0,0).
    pub const ORIGIN: Self = Self { row: 0, col: 0 };
}

/// A rectangular region within a terminal, defined by a position and size.
///
/// Useful for describing sub-regions or windows within the terminal display;
/// a region can be carved out of another with the `take_*` and `drop_*`
/// methods.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Region {
    /// The top-left position of the region.
    pub position: Position,

    /// The size (dimensions) of the region.
    pub size: Size,
}

impl Region {
    /// Returns `true` if the region has zero area (either zero rows or zero columns).
    pub const fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// Returns `true` if the given position falls within this region.
    pub const fn contains(self, position: Position) -> bool {
        position.row >= self.position.row
            && position.col >= self.position.col
            && position.row < self.position.row + self.size.rows
            && position.col < self.position.col + self.size.cols
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
