use std::ops::{Add, AddAssign, Sub, SubAssign};

/// The number of rows and columns of a [`TerminalFrame`](crate::TerminalFrame).
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

    /// Returns a new region shrunk by the specified amount from all directions.
    pub const fn drop(self, amount: usize) -> Self {
        self.drop_top(amount)
            .drop_bottom(amount)
            .drop_left(amount)
            .drop_right(amount)
    }

    /// Returns a new region expanded upward by the specified number of rows.
    /// The position moves up and the height increases.
    pub const fn expand_top(mut self, rows: usize) -> Self {
        self.position.row = self.position.row.saturating_sub(rows);
        self.size.rows = self.size.rows.saturating_add(rows);
        self
    }

    /// Returns a new region expanded downward by the specified number of rows.
    /// The height increases while the position stays the same.
    pub const fn expand_bottom(mut self, rows: usize) -> Self {
        self.size.rows = self.size.rows.saturating_add(rows);
        self
    }

    /// Returns a new region expanded leftward by the specified number of columns.
    /// The position moves left and the width increases.
    pub const fn expand_left(mut self, cols: usize) -> Self {
        self.position.col = self.position.col.saturating_sub(cols);
        self.size.cols = self.size.cols.saturating_add(cols);
        self
    }

    /// Returns a new region expanded rightward by the specified number of columns.
    /// The width increases while the position stays the same.
    pub const fn expand_right(mut self, cols: usize) -> Self {
        self.size.cols = self.size.cols.saturating_add(cols);
        self
    }

    /// Returns a new region expanded by the specified amount in all directions.
    pub const fn expand(self, amount: usize) -> Self {
        self.expand_top(amount)
            .expand_bottom(amount)
            .expand_left(amount)
            .expand_right(amount)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// `TerminalPosition::add` and `sub` must follow the documented
    /// component-wise semantics: `add` uses plain addition, `sub` uses
    /// saturating subtraction.
    #[test]
    fn pbt_position_add_sub_match_definitions() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let p = sample_pbt_position(ctx);
            let q = sample_pbt_position(ctx);
            let sum = p + q;
            assert_eq!(sum.row, p.row + q.row, "add row mismatch: {p:?} + {q:?}");
            assert_eq!(sum.col, p.col + q.col, "add col mismatch: {p:?} + {q:?}");
            let diff = p - q;
            assert_eq!(
                diff.row,
                p.row.saturating_sub(q.row),
                "sub row mismatch: {p:?} - {q:?}"
            );
            assert_eq!(
                diff.col,
                p.col.saturating_sub(q.col),
                "sub col mismatch: {p:?} - {q:?}"
            );
            Ok(())
        })?;
        Ok(())
    }

    /// The `take_*`, `drop_*`, and `expand_*` region operations must
    /// clamp by the requested amount, keep the untouched dimension, and
    /// (for take/drop) stay within the original region; `expand_*` must
    /// grow outward by the requested amount. `contains()` must agree
    /// with the resulting region.
    #[test]
    fn pbt_region_take_drop_expand_match_definitions() -> noprop::TestResult {
        let observed_take = Cell::new(false);
        let observed_drop = Cell::new(false);
        let observed_expand = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let region = sample_pbt_region(ctx);
            let amount = noprop::sample_usize_in(ctx, 0..=8);
            match noprop::sample_weighted_index(ctx, &[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]) {
                0 => {
                    observed_take.set(true);
                    let taken = region.take_top(amount);
                    assert_eq!(taken.size.rows, region.size.rows.min(amount));
                    assert_eq!(taken.position, region.position);
                    assert_eq!(taken.size.cols, region.size.cols);
                    assert_contained(ctx, region, taken);
                }
                1 => {
                    observed_take.set(true);
                    let taken = region.take_bottom(amount);
                    assert_eq!(taken.size.rows, region.size.rows.min(amount));
                    assert_eq!(taken.position.col, region.position.col);
                    assert_eq!(
                        taken.position.row,
                        region.position.row + region.size.rows - taken.size.rows
                    );
                    assert_eq!(taken.size.cols, region.size.cols);
                    assert_contained(ctx, region, taken);
                }
                2 => {
                    observed_take.set(true);
                    let taken = region.take_left(amount);
                    assert_eq!(taken.size.cols, region.size.cols.min(amount));
                    assert_eq!(taken.position, region.position);
                    assert_eq!(taken.size.rows, region.size.rows);
                    assert_contained(ctx, region, taken);
                }
                3 => {
                    observed_take.set(true);
                    let taken = region.take_right(amount);
                    assert_eq!(taken.size.cols, region.size.cols.min(amount));
                    assert_eq!(taken.position.row, region.position.row);
                    assert_eq!(
                        taken.position.col,
                        region.position.col + region.size.cols - taken.size.cols
                    );
                    assert_eq!(taken.size.rows, region.size.rows);
                    assert_contained(ctx, region, taken);
                }
                4 => {
                    observed_drop.set(true);
                    let dropped = region.drop_top(amount);
                    assert_eq!(dropped.size.rows, region.size.rows.saturating_sub(amount));
                    assert_eq!(dropped.position.col, region.position.col);
                    assert_eq!(
                        dropped.position.row,
                        if amount < region.size.rows {
                            region.position.row + amount
                        } else {
                            region.position.row
                        }
                    );
                    assert_eq!(dropped.size.cols, region.size.cols);
                    assert_contained(ctx, region, dropped);
                }
                5 => {
                    observed_drop.set(true);
                    let dropped = region.drop_bottom(amount);
                    assert_eq!(dropped.size.rows, region.size.rows.saturating_sub(amount));
                    assert_eq!(dropped.position, region.position);
                    assert_eq!(dropped.size.cols, region.size.cols);
                    assert_contained(ctx, region, dropped);
                }
                6 => {
                    observed_drop.set(true);
                    let dropped = region.drop_left(amount);
                    assert_eq!(dropped.size.cols, region.size.cols.saturating_sub(amount));
                    assert_eq!(dropped.position.row, region.position.row);
                    assert_eq!(
                        dropped.position.col,
                        if amount < region.size.cols {
                            region.position.col + amount
                        } else {
                            region.position.col
                        }
                    );
                    assert_eq!(dropped.size.rows, region.size.rows);
                    assert_contained(ctx, region, dropped);
                }
                7 => {
                    observed_drop.set(true);
                    let dropped = region.drop_right(amount);
                    assert_eq!(dropped.size.cols, region.size.cols.saturating_sub(amount));
                    assert_eq!(dropped.position, region.position);
                    assert_eq!(dropped.size.rows, region.size.rows);
                    assert_contained(ctx, region, dropped);
                }
                8 => {
                    observed_expand.set(true);
                    let expanded = region.expand_top(amount);
                    assert_eq!(expanded.size.rows, region.size.rows.saturating_add(amount));
                    assert_eq!(
                        expanded.position.row,
                        region.position.row.saturating_sub(amount)
                    );
                    assert_eq!(expanded.position.col, region.position.col);
                    assert_eq!(expanded.size.cols, region.size.cols);
                }
                9 => {
                    observed_expand.set(true);
                    let expanded = region.expand_bottom(amount);
                    assert_eq!(expanded.size.rows, region.size.rows.saturating_add(amount));
                    assert_eq!(expanded.position, region.position);
                    assert_eq!(expanded.size.cols, region.size.cols);
                }
                10 => {
                    observed_expand.set(true);
                    let expanded = region.expand_left(amount);
                    assert_eq!(expanded.size.cols, region.size.cols.saturating_add(amount));
                    assert_eq!(
                        expanded.position.col,
                        region.position.col.saturating_sub(amount)
                    );
                    assert_eq!(expanded.position.row, region.position.row);
                    assert_eq!(expanded.size.rows, region.size.rows);
                }
                _ => {
                    observed_expand.set(true);
                    let expanded = region.expand_right(amount);
                    assert_eq!(expanded.size.cols, region.size.cols.saturating_add(amount));
                    assert_eq!(expanded.position, region.position);
                    assert_eq!(expanded.size.rows, region.size.rows);
                }
            }
            Ok(())
        })?;
        assert!(
            observed_take.get(),
            "no case exercised a take_* operation\n{runner}"
        );
        assert!(
            observed_drop.get(),
            "no case exercised a drop_* operation\n{runner}"
        );
        assert!(
            observed_expand.get(),
            "no case exercised an expand_* operation\n{runner}"
        );
        Ok(())
    }

    fn sample_pbt_position(ctx: &mut noprop::TestCaseContext) -> TerminalPosition {
        let row = noprop::sample_with_boundaries(
            ctx,
            &[0usize, 1000],
            noprop::Ratio::one_nth(5),
            |ctx| noprop::sample_usize_in(ctx, 0..=1000),
        );
        let col = noprop::sample_with_boundaries(
            ctx,
            &[0usize, 1000],
            noprop::Ratio::one_nth(5),
            |ctx| noprop::sample_usize_in(ctx, 0..=1000),
        );
        TerminalPosition::row_col(row, col)
    }

    fn sample_pbt_region(ctx: &mut noprop::TestCaseContext) -> TerminalRegion {
        let row =
            noprop::sample_with_boundaries(ctx, &[0usize, 8], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=8)
            });
        let col =
            noprop::sample_with_boundaries(ctx, &[0usize, 8], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=8)
            });
        let rows =
            noprop::sample_with_boundaries(ctx, &[0usize, 8], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=8)
            });
        let cols =
            noprop::sample_with_boundaries(ctx, &[0usize, 8], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=8)
            });
        TerminalRegion {
            position: TerminalPosition::row_col(row, col),
            size: TerminalSize::rows_cols(rows, cols),
        }
    }

    /// A position sampled inside `extracted` must also be inside
    /// `original` — the take_*/drop_* regions are sub-regions of the
    /// original. Empty regions have no positions so the check is
    /// vacuous.
    fn assert_contained(
        ctx: &mut noprop::TestCaseContext,
        original: TerminalRegion,
        extracted: TerminalRegion,
    ) {
        if extracted.size.is_empty() {
            return;
        }
        let row = extracted.position.row + noprop::sample_usize_in(ctx, 0..extracted.size.rows);
        let col = extracted.position.col + noprop::sample_usize_in(ctx, 0..extracted.size.cols);
        let p = TerminalPosition::row_col(row, col);
        assert!(
            original.contains(p),
            "extracted region must be within original:\n  original = {original:?}\n  extracted = {extracted:?}\n  sample = {p:?}"
        );
    }
}
