//! Property-based tests for tuinix, driven by noprop.
//!
//! The properties covered here use only the public API:
//!
//! - A [`TerminalStyle`] emitted via `Display` parses back to the
//!   original style (`FromStr` round-trip).
//! - The `take` / `drop` / `expand` operations of [`TerminalRegion`]
//!   match an independent `(position, size)` model, and `contains`
//!   agrees with a cell-set model on random probe points.
//! - The cursor of [`TerminalFrame`] after `push_char` / `push_newline`
//!   matches a model: a cell advances the cursor by its width, and `\n`
//!   resets the column, regardless of clipping.

use std::cell::Cell;
use std::collections::BTreeSet;

/// Runs a property with a time-based seed, overridable via the
/// `TUINIX_PBT_SEED` environment variable for deterministic
/// reproduction of a reported failure. The runner is returned so that
/// coverage-gate assertions can embed its seed in the failure message.
fn run<F>(cases: usize, f: F) -> noprop::TestResult<noprop::Runner>
where
    F: Fn(&mut noprop::TestCaseContext) -> noprop::TestResult,
{
    let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(cases, f)?;
    Ok(runner)
}

fn sample_color(ctx: &mut noprop::TestCaseContext) -> tuinix::TerminalColor {
    tuinix::TerminalColor::new(
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
    )
}

/// Draws a random `TerminalStyle`, including the reset style.
fn sample_style(ctx: &mut noprop::TestCaseContext) -> tuinix::TerminalStyle {
    const SETTERS: [fn(tuinix::TerminalStyle) -> tuinix::TerminalStyle; 7] = [
        tuinix::TerminalStyle::bold,
        tuinix::TerminalStyle::italic,
        tuinix::TerminalStyle::underline,
        tuinix::TerminalStyle::blink,
        tuinix::TerminalStyle::reverse,
        tuinix::TerminalStyle::dim,
        tuinix::TerminalStyle::strikethrough,
    ];
    let mut style = tuinix::TerminalStyle::new();
    for setter in SETTERS {
        if noprop::sample_bool(ctx) {
            style = setter(style);
        }
    }
    if noprop::sample_bool(ctx) {
        style = style.fg_color(sample_color(ctx));
    }
    if noprop::sample_bool(ctx) {
        style = style.bg_color(sample_color(ctx));
    }
    style
}

/// Every `TerminalStyle` must round-trip through its ANSI escape
/// sequence representation.
#[test]
fn style_roundtrip_matches_display() -> noprop::TestResult {
    let observed_styled = Cell::new(false);
    let observed_fg = Cell::new(false);
    let observed_bg = Cell::new(false);
    let runner = run(256, |ctx| {
        let style = sample_style(ctx);
        let text = style.to_string();
        let parsed = text
            .parse::<tuinix::TerminalStyle>()
            .unwrap_or_else(|e| panic!("{style:?} emitted {text:?}, which fails to parse: {e}"));
        assert_eq!(parsed, style, "style round-trip mismatch");
        if style != tuinix::TerminalStyle::new() {
            observed_styled.set(true);
        }
        if style.fg_color.is_some() {
            observed_fg.set(true);
        }
        if style.bg_color.is_some() {
            observed_bg.set(true);
        }
        Ok(())
    })?;
    assert!(
        observed_styled.get(),
        "no case exercised a styled style\n{runner}"
    );
    assert!(observed_fg.get(), "no case exercised an fg color\n{runner}");
    assert!(observed_bg.get(), "no case exercised a bg color\n{runner}");
    Ok(())
}

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

/// An independent `(position, size)` model of region arithmetic,
/// written without the `TerminalRegion` method chain under test.
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

/// The `take` / `drop` / `expand` operations of `TerminalRegion` must
/// agree with the `(position, size)` model after every step, and
/// `contains` must agree with the cell-set model on a random probe.
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

/// A single write operation on a [`TerminalFrame`]: a styled cell with an
/// explicit width, or a newline.
#[derive(Debug, Clone, Copy)]
enum Op {
    Newline,
    Cell(char, usize),
}

fn sample_op(ctx: &mut noprop::TestCaseContext) -> Op {
    match noprop::sample_weighted_index(ctx, &[1, 8]) {
        0 => Op::Newline,
        _ => {
            let c = char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                .expect("valid ASCII");
            let width = noprop::sample_choice(ctx, &[1usize, 1, 1, 2]);
            Op::Cell(c, width)
        }
    }
}

/// A cursor-position model for [`Op`]: a cell advances the column by
/// its width, and `\n` resets the column, regardless of clipping.
#[derive(Debug)]
struct CursorModel {
    row: usize,
    col: usize,
    clipped: bool,
}

impl CursorModel {
    fn apply(&mut self, op: Op, size: tuinix::TerminalSize) {
        match op {
            Op::Newline => {
                self.row += 1;
                self.col = 0;
            }
            Op::Cell(_, width) => {
                if width == 0 {
                    return;
                }
                if self.row >= size.rows || self.col + width > size.cols {
                    self.clipped = true;
                }
                self.col += width;
            }
        }
    }
}

/// The cursor of `TerminalFrame` must follow the model after applying
/// a random sequence of cell writes and newlines.
#[test]
fn frame_push_cursor_matches_model() -> noprop::TestResult {
    let observed_cell = Cell::new(false);
    let observed_wide = Cell::new(false);
    let observed_newline = Cell::new(false);
    let observed_clipped = Cell::new(false);
    let runner = run(256, |ctx| {
        let size = tuinix::TerminalSize::rows_cols(
            noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=12)
            }),
            noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=12)
            }),
        );
        let mut ops = Vec::new();
        let n_ops =
            noprop::sample_with_boundaries(ctx, &[0usize, 64], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=64)
            });
        for _ in 0..n_ops {
            ops.push(sample_op(ctx));
        }
        let mut model = CursorModel {
            row: 0,
            col: 0,
            clipped: false,
        };
        let mut frame: tuinix::TerminalFrame = tuinix::TerminalFrame::new(size);
        for &op in &ops {
            model.apply(op, size);
            match op {
                Op::Newline => frame.push_newline(),
                Op::Cell(c, width) => {
                    frame.push_char(
                        tuinix::TerminalChar::new(c, width, tuinix::TerminalStyle::new())
                            .expect("valid cell"),
                    );
                }
            }
        }
        assert_eq!(
            frame.cursor(),
            tuinix::TerminalPosition::row_col(model.row, model.col),
            "cursor mismatch for {ops:?}"
        );
        if ops.iter().any(|op| matches!(op, Op::Cell(_, w) if *w > 0)) {
            observed_cell.set(true);
        }
        if ops.iter().any(|op| matches!(op, Op::Cell(_, w) if *w == 2)) {
            observed_wide.set(true);
        }
        if ops.iter().any(|op| matches!(op, Op::Newline)) {
            observed_newline.set(true);
        }
        if model.clipped {
            observed_clipped.set(true);
        }
        Ok(())
    })?;
    assert!(observed_cell.get(), "no case wrote a cell\n{runner}");
    assert!(observed_wide.get(), "no case wrote a wide cell\n{runner}");
    assert!(observed_newline.get(), "no case wrote a newline\n{runner}");
    assert!(observed_clipped.get(), "no case clipped a cell\n{runner}");
    Ok(())
}
