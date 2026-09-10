//! Property-based tests for the geometry types, driven by noprop.
//!
//! The properties covered here use only the public API:
//!
//! - `TerminalPosition::add` and `sub` follow the documented component-wise
//!   semantics (plain addition, saturating subtraction).
//! - The `take` / `drop` / `expand` operations of `TerminalRegion` match an
//!   independent `(position, size)` model, and `contains` agrees with a
//!   cell-set model on random probe points.

mod common;

use std::cell::Cell;
use std::collections::BTreeSet;

use common::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionOp {
    TakeTop,
    TakeBottom,
    TakeLeft,
    TakeRight,
    DropTop,
    DropBottom,
    DropLeft,
    DropRight,
    ExpandTop,
    ExpandBottom,
    ExpandLeft,
    ExpandRight,
}

const REGION_OPS: [RegionOp; 12] = [
    RegionOp::TakeTop,
    RegionOp::TakeBottom,
    RegionOp::TakeLeft,
    RegionOp::TakeRight,
    RegionOp::DropTop,
    RegionOp::DropBottom,
    RegionOp::DropLeft,
    RegionOp::DropRight,
    RegionOp::ExpandTop,
    RegionOp::ExpandBottom,
    RegionOp::ExpandLeft,
    RegionOp::ExpandRight,
];

/// An independent `(position, size)` model of region arithmetic, written
/// without the `TerminalRegion` method chain under test.
#[derive(Debug, Clone, Copy)]
struct RegionModel {
    position: tuinix::TerminalPosition,
    size: tuinix::TerminalSize,
}

impl RegionModel {
    fn apply(self, op: RegionOp, n: usize) -> Self {
        let (mut position, mut size) = (self.position, self.size);
        match op {
            RegionOp::TakeTop => {
                size.rows = size.rows.min(n);
            }
            RegionOp::TakeBottom => {
                if n < size.rows {
                    position.row += size.rows - n;
                    size.rows = n;
                }
            }
            RegionOp::TakeLeft => {
                size.cols = size.cols.min(n);
            }
            RegionOp::TakeRight => {
                if n < size.cols {
                    position.col += size.cols - n;
                    size.cols = n;
                }
            }
            RegionOp::DropTop => {
                if n < size.rows {
                    position.row += n;
                    size.rows -= n;
                } else {
                    size.rows = 0;
                }
            }
            RegionOp::DropBottom => {
                size.rows = size.rows.saturating_sub(n);
            }
            RegionOp::DropLeft => {
                if n < size.cols {
                    position.col += n;
                    size.cols -= n;
                } else {
                    size.cols = 0;
                }
            }
            RegionOp::DropRight => {
                size.cols = size.cols.saturating_sub(n);
            }
            RegionOp::ExpandTop => {
                position.row = position.row.saturating_sub(n);
                size.rows = size.rows.saturating_add(n);
            }
            RegionOp::ExpandBottom => {
                size.rows = size.rows.saturating_add(n);
            }
            RegionOp::ExpandLeft => {
                position.col = position.col.saturating_sub(n);
                size.cols = size.cols.saturating_add(n);
            }
            RegionOp::ExpandRight => {
                size.cols = size.cols.saturating_add(n);
            }
        }
        Self { position, size }
    }

    fn cells(self) -> BTreeSet<tuinix::TerminalPosition> {
        let mut cells = BTreeSet::new();
        for row in self.position.row..self.position.row + self.size.rows {
            for col in self.position.col..self.position.col + self.size.cols {
                cells.insert(tuinix::TerminalPosition::row_col(row, col));
            }
        }
        cells
    }
}

fn apply_op(region: tuinix::TerminalRegion, op: RegionOp, n: usize) -> tuinix::TerminalRegion {
    match op {
        RegionOp::TakeTop => region.take_top(n),
        RegionOp::TakeBottom => region.take_bottom(n),
        RegionOp::TakeLeft => region.take_left(n),
        RegionOp::TakeRight => region.take_right(n),
        RegionOp::DropTop => region.drop_top(n),
        RegionOp::DropBottom => region.drop_bottom(n),
        RegionOp::DropLeft => region.drop_left(n),
        RegionOp::DropRight => region.drop_right(n),
        RegionOp::ExpandTop => region.expand_top(n),
        RegionOp::ExpandBottom => region.expand_bottom(n),
        RegionOp::ExpandLeft => region.expand_left(n),
        RegionOp::ExpandRight => region.expand_right(n),
    }
}

fn sample_amount(ctx: &mut noprop::TestCaseContext) -> usize {
    noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
        noprop::sample_usize_in(ctx, 0..=12)
    })
}

/// The `take` / `drop` / `expand` operations of `TerminalRegion` must agree
/// with the `(position, size)` model after every step, and `contains` must
/// agree with the cell-set model on a random probe.
#[test]
fn region_operations_match_model() -> noprop::TestResult {
    let observed = REGION_OPS.map(|_| Cell::new(false));
    let observed_empty = Cell::new(false);
    let observed_zero = Cell::new(false);
    let observed_max = Cell::new(false);
    let runner = run(256, |ctx| {
        let position = tuinix::TerminalPosition::row_col(
            noprop::sample_usize_in(ctx, 0..=10),
            noprop::sample_usize_in(ctx, 0..=10),
        );
        let size = tuinix::TerminalSize::rows_cols(
            noprop::sample_usize_in(ctx, 0..=10),
            noprop::sample_usize_in(ctx, 0..=10),
        );
        let mut region = tuinix::TerminalRegion { position, size };
        let mut model = RegionModel { position, size };
        let steps =
            noprop::sample_with_boundaries(ctx, &[1usize, 32], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 1..=32)
            });
        for _ in 0..steps {
            let op = REGION_OPS[noprop::sample_usize_in(ctx, 0..REGION_OPS.len())];
            let n = sample_amount(ctx);
            region = apply_op(region, op, n);
            model = model.apply(op, n);
            assert_eq!(
                (region.position, region.size),
                (model.position, model.size),
                "{op:?}({n}) mismatch"
            );
            let probe = tuinix::TerminalPosition::row_col(
                noprop::sample_usize_in(ctx, 0..=20),
                noprop::sample_usize_in(ctx, 0..=20),
            );
            assert_eq!(
                region.contains(probe),
                model.cells().contains(&probe),
                "contains({probe:?}) mismatch"
            );
            observed[op as usize].set(true);
            if region.is_empty() {
                observed_empty.set(true);
            }
            if n == 0 {
                observed_zero.set(true);
            }
            if n == 12 {
                observed_max.set(true);
            }
        }
        Ok(())
    })?;
    for (op, gate) in REGION_OPS.iter().zip(&observed) {
        assert!(gate.get(), "no case exercised {op:?}\n{runner}");
    }
    assert!(
        observed_empty.get(),
        "no case produced an empty region\n{runner}"
    );
    assert!(observed_zero.get(), "no case used amount 0\n{runner}");
    assert!(observed_max.get(), "no case used the max amount\n{runner}");
    Ok(())
}

/// Samples a position with a bias toward the boundaries of the range.
fn sample_position(ctx: &mut noprop::TestCaseContext) -> tuinix::TerminalPosition {
    let row =
        noprop::sample_with_boundaries(ctx, &[0usize, 1000], noprop::Ratio::one_nth(5), |ctx| {
            noprop::sample_usize_in(ctx, 0..=1000)
        });
    let col =
        noprop::sample_with_boundaries(ctx, &[0usize, 1000], noprop::Ratio::one_nth(5), |ctx| {
            noprop::sample_usize_in(ctx, 0..=1000)
        });
    tuinix::TerminalPosition::row_col(row, col)
}

/// `TerminalPosition::add` adds component-wise and `sub` saturates
/// component-wise, as documented.
#[test]
fn position_add_sub_match_definitions() -> noprop::TestResult {
    run(256, |ctx| {
        let p = sample_position(ctx);
        let q = sample_position(ctx);
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
