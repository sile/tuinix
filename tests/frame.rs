//! Property-based tests for `Frame`, driven by noprop.
//!
//! The properties covered here use only the public API:
//!
//! - The write position after `push_char` / `push_newline` matches a model:
//!   a character advances the position by its width, and `\n` resets the
//!   column, regardless of clipping.
//! - `draw` matches a model that replays the overlap handling: a partially
//!   overlapped character is removed, the cells a drawn character covers are
//!   cleared, and characters drawn outside the frame are ignored.
//! - Replaying the bytes produced by `render` onto a model screen reproduces
//!   `chars()`, both without a previous frame (a full redraw) and with one (a
//!   differential update).

mod helpers;

use std::cell::Cell;
use std::collections::BTreeMap;

use helpers::run;

/// Characters the tests treat as two columns wide.
const WIDE_CHARS: &[char] = &['\u{3042}', '\u{754c}', '\u{65e5}'];

/// Characters the tests treat as zero columns wide.
const ZERO_WIDTH_CHARS: &[char] = &['\u{301}', '\u{20dd}'];

/// The bytes every render starts with, hiding the cursor.
const HIDE_CURSOR: &[u8] = b"\x1b[?25l";

/// The width the tests assign to `c`.
///
/// A frame never computes widths itself, so the tests pick a table of their
/// own: the two constants above, and one column for everything else.
fn char_width(c: char) -> usize {
    if c.is_control() {
        0
    } else if WIDE_CHARS.contains(&c) {
        2
    } else if ZERO_WIDTH_CHARS.contains(&c) {
        0
    } else {
        1
    }
}

/// Builds a valid character for the tests.
fn ch(value: char, width: usize) -> tuinix::Char {
    tuinix::Char::new(value, width, tuinix::Style::new()).expect("valid char")
}

/// Pushes a string of text onto the frame, handling newlines and per-character
/// widths. A zero-width character occupies no column, so it is dropped.
fn push_text(frame: &mut tuinix::Frame, text: &str) {
    for c in text.chars() {
        match c {
            '\n' => frame.push_newline(),
            _ => {
                let width = char_width(c);
                if width > 0 {
                    frame.push_char(ch(c, width));
                }
            }
        }
    }
}

fn sample_size(ctx: &mut noprop::TestCaseContext) -> tuinix::Size {
    tuinix::Size::rows_cols(
        noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
            noprop::sample_usize_in(ctx, 0..=12)
        }),
        noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
            noprop::sample_usize_in(ctx, 0..=12)
        }),
    )
}

fn sample_text(ctx: &mut noprop::TestCaseContext) -> String {
    let mut text = String::new();
    let n_chars =
        noprop::sample_with_boundaries(ctx, &[0usize, 48], noprop::Ratio::one_nth(5), |ctx| {
            noprop::sample_usize_in(ctx, 0..=48)
        });
    for _ in 0..n_chars {
        match noprop::sample_weighted_index(ctx, &[4, 1, 1, 1]) {
            0 => {
                text.push(
                    char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                        .expect("valid ASCII"),
                );
            }
            1 => text.push('\n'),
            2 => text.push(noprop::sample_choice(ctx, WIDE_CHARS)),
            _ => text.push(noprop::sample_choice(ctx, ZERO_WIDTH_CHARS)),
        }
    }
    text
}

/// A single write operation on a `Frame`: a styled character with an
/// explicit width, or a newline.
#[derive(Debug, Clone, Copy)]
enum Op {
    Newline,
    Char(char, usize),
}

fn sample_op(ctx: &mut noprop::TestCaseContext) -> Op {
    match noprop::sample_weighted_index(ctx, &[1, 8]) {
        0 => Op::Newline,
        _ => {
            let c = char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                .expect("valid ASCII");
            let width = noprop::sample_choice(ctx, &[1usize, 1, 1, 2]);
            Op::Char(c, width)
        }
    }
}

/// A write-position model for `Op`: a character advances the column by its
/// width, and `\n` resets the column, regardless of clipping.
#[derive(Debug)]
struct CursorModel {
    row: usize,
    col: usize,
    clipped: bool,
}

impl CursorModel {
    fn apply(&mut self, op: Op, size: tuinix::Size) {
        match op {
            Op::Newline => {
                self.row += 1;
                self.col = 0;
            }
            Op::Char(_, width) => {
                if self.row >= size.rows || self.col + width > size.cols {
                    self.clipped = true;
                }
                self.col += width;
            }
        }
    }
}

/// The write position must follow the model after applying a random sequence
/// of character writes and newlines.
#[test]
fn push_cursor_matches_model() -> noprop::TestResult {
    let observed_char = Cell::new(false);
    let observed_wide = Cell::new(false);
    let observed_newline = Cell::new(false);
    let observed_clipped = Cell::new(false);
    let runner = run(256, |ctx| {
        let size = tuinix::Size::rows_cols(
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
        let mut frame = tuinix::Frame::new(size);
        for &op in &ops {
            model.apply(op, size);
            match op {
                Op::Newline => frame.push_newline(),
                Op::Char(c, width) => {
                    frame.push_char(ch(c, width));
                }
            }
        }
        assert_eq!(
            frame.next_push_position(),
            tuinix::Position::row_col(model.row, model.col),
            "write position mismatch for {ops:?}"
        );
        if ops.iter().any(|op| matches!(op, Op::Char(_, w) if *w > 0)) {
            observed_char.set(true);
        }
        if ops.iter().any(|op| matches!(op, Op::Char(_, w) if *w == 2)) {
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
    assert!(observed_char.get(), "no case wrote a character\n{runner}");
    assert!(
        observed_wide.get(),
        "no case wrote a wide character\n{runner}"
    );
    assert!(observed_newline.get(), "no case wrote a newline\n{runner}");
    assert!(
        observed_clipped.get(),
        "no case clipped a character\n{runner}"
    );
    Ok(())
}

/// `draw` must match a model that replays the overlap handling: a partially
/// overlapped character is removed, the cells covered by the drawn character
/// are cleared, and characters drawn outside the frame are ignored.
#[test]
fn draw_matches_model() -> noprop::TestResult {
    let observed_overlap = Cell::new(false);
    let observed_clipped = Cell::new(false);
    let runner = run(256, |ctx| {
        // Half of the cases force a partial overlap structurally: a wide character
        // whose second cell is overwritten by a drawn character.
        let structured = noprop::sample_bool(ctx);
        let (size, dest_text, src_text, position) = if structured {
            (
                tuinix::Size::rows_cols(1, 4),
                "\u{3042}".to_string(),
                "x".to_string(),
                tuinix::Position::row_col(0, 1),
            )
        } else {
            (
                sample_size(ctx),
                sample_text(ctx),
                sample_text(ctx),
                tuinix::Position::row_col(
                    noprop::sample_usize_in(ctx, 0..=16),
                    noprop::sample_usize_in(ctx, 0..=16),
                ),
            )
        };

        let mut dest = tuinix::Frame::new(size);
        push_text(&mut dest, &dest_text);
        let mut src = tuinix::Frame::new(size);
        push_text(&mut src, &src_text);

        let mut expected: BTreeMap<_, _> = dest
            .chars()
            .filter(|(_, c)| *c != tuinix::Char::BLANK)
            .collect();
        let mut removals = 0usize;
        let mut skipped = 0usize;
        for (src_pos, c) in src.chars() {
            let target_pos = position + src_pos;
            if target_pos.row >= size.rows || target_pos.col + c.width() > size.cols {
                skipped += 1;
                continue;
            }
            if let Some((&prev_pos, prev_c)) = expected.range(..target_pos).next_back() {
                let end_pos = prev_pos + tuinix::Position::col(prev_c.width());
                if target_pos < end_pos {
                    expected.remove(&prev_pos);
                    removals += 1;
                }
            }
            for i in 0..c.width() {
                expected.remove(&(target_pos + tuinix::Position::col(i)));
            }
            expected.insert(target_pos, c);
        }

        dest.draw(position, &src);
        let actual: BTreeMap<_, _> = dest
            .chars()
            .filter(|(_, c)| *c != tuinix::Char::BLANK)
            .collect();
        let expected: BTreeMap<_, _> = expected
            .into_iter()
            .filter(|(_, c)| *c != tuinix::Char::BLANK)
            .collect();
        assert_eq!(actual, expected, "draw mismatch at {position:?}");
        if removals > 0 {
            observed_overlap.set(true);
        }
        if skipped > 0 {
            observed_clipped.set(true);
        }
        Ok(())
    })?;
    assert!(
        observed_overlap.get(),
        "no case removed an overlapped character\n{runner}"
    );
    assert!(
        observed_clipped.get(),
        "no case drew outside the frame\n{runner}"
    );
    Ok(())
}

/// Replays the byte stream produced by `render` onto a model screen.
///
/// It understands cursor positioning (`ESC [ <row> ; <col> H`), cursor
/// visibility (`ESC [ ? 25 h` / `l`), and SGR style sequences
/// (`ESC [ ... m`), which are ignored. Any other character is written at the
/// cursor, overwriting the cells it covers and advancing the cursor by its
/// width, the way a terminal does.
#[derive(Debug)]
struct ScreenModel {
    cells: BTreeMap<tuinix::Position, char>,
    cursor: tuinix::Position,
    writes: usize,
}

impl ScreenModel {
    fn new() -> Self {
        Self {
            cells: BTreeMap::new(),
            cursor: tuinix::Position::ZERO,
            writes: 0,
        }
    }

    fn apply(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some((&first, remainder)) = rest.split_first() {
            if first == 0x1b {
                assert_eq!(remainder.first(), Some(&b'['), "malformed escape sequence");
                let end = remainder[1..]
                    .iter()
                    .position(|b| matches!(b, b'H' | b'm' | b'h' | b'l'))
                    .expect("terminated escape sequence")
                    + 1;
                let params = std::str::from_utf8(&remainder[1..end]).expect("ASCII parameters");
                match remainder[end] {
                    b'H' => {
                        let (row, col) = params.split_once(';').expect("row;col");
                        self.cursor = tuinix::Position::row_col(
                            row.parse::<usize>().expect("row") - 1,
                            col.parse::<usize>().expect("col") - 1,
                        );
                    }
                    b'h' | b'l' => assert_eq!(params, "?25", "unexpected mode change"),
                    _ => {} // SGR style sequence: does not move the cursor.
                }
                rest = &remainder[end + 1..];
                continue;
            }

            let c = std::str::from_utf8(rest)
                .expect("valid UTF-8")
                .chars()
                .next()
                .expect("non-empty");
            let width = char_width(c);
            assert!(
                width >= 1,
                "zero-width character {c:?} in the renderer output"
            );

            // Writing a character overwrites the cells it covers, and it also
            // invalidates a wide character written to its left that spans into
            // the new character's first cell.
            if let Some((&pos, prev)) = self.cells.range(..self.cursor).next_back()
                && self.cursor < pos + tuinix::Position::col(char_width(*prev))
            {
                self.cells.remove(&pos);
            }
            for i in 0..width {
                self.cells.remove(&(self.cursor + tuinix::Position::col(i)));
            }
            self.cells.insert(self.cursor, c);
            self.cursor.col += width;
            self.writes += 1;
            rest = &rest[c.len_utf8()..];
        }
    }

    /// The cells that lie inside `size`.
    ///
    /// After the terminal shrinks, cells the previous frame painted outside the
    /// new area stay in the model but are no longer visible, so only the cells
    /// inside the current size are compared.
    fn visible_cells(&self, size: tuinix::Size) -> BTreeMap<tuinix::Position, char> {
        self.cells
            .iter()
            .filter(|(pos, _)| pos.row < size.rows && pos.col < size.cols)
            .map(|(pos, c)| (*pos, *c))
            .collect()
    }
}

/// The characters a frame paints, including the blanks of unwritten positions.
fn rendered_cells(frame: &tuinix::Frame) -> BTreeMap<tuinix::Position, char> {
    frame.chars().map(|(pos, c)| (pos, c.value())).collect()
}

/// Rendering without a previous frame must paint every cell of the frame,
/// blanks included, so replaying the output reproduces `chars()`.
#[test]
fn render_full_redraw_matches_model() -> noprop::TestResult {
    let observed_wide = Cell::new(false);
    let observed_blank = Cell::new(false);
    let runner = run(256, |ctx| {
        let size = sample_size(ctx);
        let mut frame = tuinix::Frame::new(size);
        push_text(&mut frame, &sample_text(ctx));

        let out = frame.render(None, None);
        assert!(out.starts_with(HIDE_CURSOR), "output must hide the cursor");

        let mut screen = ScreenModel::new();
        screen.apply(&out);
        assert_eq!(
            screen.cells,
            rendered_cells(&frame),
            "full redraw mismatch for {size:?}"
        );
        assert_eq!(
            screen.writes,
            frame.chars().count(),
            "a full redraw must write every cell exactly once"
        );

        for c in frame.chars().map(|(_, c)| c) {
            if c.width() > 1 {
                observed_wide.set(true);
            }
            if c.is_blank() {
                observed_blank.set(true);
            }
        }
        Ok(())
    })?;
    assert!(
        observed_wide.get(),
        "no case rendered a wide character\n{runner}"
    );
    assert!(
        observed_blank.get(),
        "no case rendered a blank cell\n{runner}"
    );
    Ok(())
}

/// Rendering against a previous frame must leave the terminal showing this
/// frame: replaying the difference onto the screen the previous frame painted
/// reproduces `chars()`, and a size change forces a full redraw.
#[test]
fn render_diff_matches_model() -> noprop::TestResult {
    let observed_skip = Cell::new(false);
    let observed_resize = Cell::new(false);
    let observed_cursor = Cell::new(false);
    let runner = run(256, |ctx| {
        let size = sample_size(ctx);
        // Half of the cases keep the size unchanged so that the differential path
        // is exercised; the rest may resize.
        let prev_size = if noprop::sample_bool(ctx) {
            size
        } else {
            sample_size(ctx)
        };
        let mut prev = tuinix::Frame::new(prev_size);
        push_text(&mut prev, &sample_text(ctx));

        let mut frame = tuinix::Frame::new(size);
        push_text(&mut frame, &sample_text(ctx));

        let cursor = if !size.is_empty() && noprop::sample_bool(ctx) {
            observed_cursor.set(true);
            Some(tuinix::Position::row_col(
                noprop::sample_usize_in(ctx, 0..size.rows),
                noprop::sample_usize_in(ctx, 0..size.cols),
            ))
        } else {
            None
        };

        // The screen the previous frame left behind.
        let mut screen = ScreenModel::new();
        screen.apply(&prev.render(None, None));

        let out = frame.render(Some(&prev), cursor);
        assert!(out.starts_with(HIDE_CURSOR), "output must hide the cursor");
        let writes_before = screen.writes;
        screen.apply(&out);
        assert_eq!(
            screen.visible_cells(size),
            rendered_cells(&frame),
            "diff mismatch against a previous frame of size {:?}",
            prev.size()
        );

        let full = frame.render(None, cursor);
        if prev.size() != size {
            assert_eq!(out, full, "a size change must force a full redraw");
            observed_resize.set(true);
        } else {
            // The differential path took effect when it wrote fewer characters
            // than a full redraw of the same frame would have.
            let mut full_screen = ScreenModel::new();
            full_screen.apply(&full);
            if screen.writes - writes_before < full_screen.writes {
                observed_skip.set(true);
            }
        }
        Ok(())
    })?;
    assert!(
        observed_skip.get(),
        "no case took the differential path\n{runner}"
    );
    assert!(
        observed_resize.get(),
        "no case rendered against a resized frame\n{runner}"
    );
    assert!(
        observed_cursor.get(),
        "no case positioned the cursor\n{runner}"
    );
    Ok(())
}
