use std::collections::BTreeMap;

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

/// A single styled cell in a [`TerminalFrame`].
///
/// `width` is the number of terminal columns the cell occupies. A stored cell must have
/// a width of at least `1`; a width of `0` is reserved for zero-width (combining)
/// characters, which are dropped by [`TerminalFrame::push_cell`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalChar {
    /// The character displayed by this cell.
    pub value: char,

    /// The number of terminal columns this cell occupies (`1` or more).
    pub width: usize,

    /// The style applied to this cell.
    pub style: TerminalStyle,
}

impl TerminalChar {
    /// A blank cell (a single space with no styling).
    pub const BLANK: Self = Self::new(' ', 1, TerminalStyle::new());

    /// Makes a new styled cell with the given width.
    pub const fn new(value: char, width: usize, style: TerminalStyle) -> Self {
        Self {
            value,
            width,
            style,
        }
    }
}

/// A frame buffer representing the terminal display state.
///
/// [`TerminalFrame`] is a concrete, width-agnostic buffer of styled cells. Each cell
/// stores the character, the number of terminal columns it occupies, and the style.
///
/// The caller supplies the correct width for each character: the frame itself never
/// computes character widths, so the library stays free of any character-width
/// dependency (such as `unicode-width`).
///
/// Cells are written with [`push_char`](Self::push_char) / [`push_cell`](Self::push_cell)
/// and advanced sequentially from an internal cursor. Use [`push_newline`](Self::push_newline)
/// to move to the next line and [`push_tab`](Self::push_tab) to advance to a tab stop.
/// A frame can be composed onto another with [`draw`](Self::draw), and its contents
/// inspected with [`chars`](Self::chars).
///
/// # Examples
///
/// ```
/// use tuinix::{TerminalChar, TerminalFrame, TerminalSize, TerminalStyle};
///
/// let size = TerminalSize::rows_cols(24, 80);
/// let mut frame = TerminalFrame::new(size);
///
/// let bold = TerminalStyle::new().bold();
/// frame.push_char('H', 1, bold);
/// frame.push_char('i', 1, bold);
/// frame.push_cell(TerminalChar::new('!', 1, bold));
/// frame.push_newline();
///
/// // A full-width (CJK) character occupies two columns.
/// frame.push_char('\u{3042}', 2, TerminalStyle::new());
///
/// assert_eq!(frame.cursor().col, 2);
/// ```
#[derive(Debug, Default, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    data: BTreeMap<TerminalPosition, TerminalChar>,
    tail: TerminalPosition,
}

impl TerminalFrame {
    /// Makes a new, empty frame with the given size.
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
            tail: TerminalPosition::ZERO,
        }
    }

    /// Returns the size of this frame.
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Returns the current cursor position (where the next cell would be written).
    pub fn cursor(&self) -> TerminalPosition {
        self.tail
    }

    /// Writes a single styled cell at the current cursor and advances the cursor.
    ///
    /// `width` is the number of terminal columns the character occupies: `2` for
    /// full-width characters (e.g. CJK, emoji), `1` for normal characters. A width of
    /// `0` is treated as a zero-width (combining) character and is silently dropped
    /// (combining marks are not yet supported).
    ///
    /// `value` must be a printable character. Control characters (newline, tab, etc.)
    /// are not cells: use [`push_newline`](Self::push_newline) and
    /// [`push_tab`](Self::push_tab) for those. Passing a control character here is a
    /// no-op (and panics in debug builds).
    ///
    /// Cells that do not fit within the frame are clipped: they are not stored, but the
    /// cursor still advances by `width`, matching terminal wrapping semantics.
    pub fn push_char(&mut self, value: char, width: usize, style: TerminalStyle) {
        if width == 0 {
            return;
        }
        if value.is_control() {
            debug_assert!(
                !value.is_control(),
                "push_char accepts only printable characters (use push_newline / push_tab)"
            );
            return;
        }
        if self.tail.row < self.size.rows && self.tail.col + width <= self.size.cols {
            self.data.insert(
                self.tail,
                TerminalChar {
                    value,
                    width,
                    style,
                },
            );
        }
        self.tail.col += width;
    }

    /// Writes a prebuilt cell at the current cursor and advances the cursor.
    pub fn push_cell(&mut self, cell: TerminalChar) {
        self.push_char(cell.value, cell.width, cell.style);
    }

    /// Moves the cursor to the beginning of the next line.
    ///
    /// Content already written is not cleared or shifted; this only moves the write
    /// cursor.
    pub fn push_newline(&mut self) {
        self.tail.row += 1;
        self.tail.col = 0;
    }

    /// Moves the cursor to the next tab stop.
    ///
    /// Tab stops are placed every `tab_width` columns, starting at column `0`. This only
    /// moves the cursor: the columns that are skipped are left blank and need not be
    /// written explicitly (as with [`push_newline`](Self::push_newline), existing content
    /// is neither cleared nor shifted).
    ///
    /// As with [`push_char`](Self::push_char), the cursor may be advanced past the right
    /// edge of the frame; use [`push_newline`](Self::push_newline) to wrap.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `tab_width` is `0`.
    pub fn push_tab(&mut self, tab_width: usize) {
        debug_assert!(tab_width > 0, "tab_width must be greater than zero");
        if tab_width == 0 {
            return;
        }
        let col = self.tail.col;
        self.tail.col += (tab_width - col % tab_width) % tab_width;
        if self.tail.col == col {
            self.tail.col += tab_width;
        }
    }

    /// Draws the contents of another frame onto this one at the given position.
    ///
    /// Characters that fall outside this frame are ignored. A character that partially
    /// overlaps a wide character causes that wide character to be removed, so its cells
    /// are not left behind as a partial glyph.
    pub fn draw(&mut self, position: TerminalPosition, frame: &TerminalFrame) {
        for (src_pos, c) in frame.chars() {
            let target_pos = position + src_pos;
            if !self.size.contains(target_pos) {
                continue;
            }

            if let Some((&prev_pos, prev_c)) = self.data.range(..target_pos).next_back() {
                let end_pos = prev_pos + TerminalPosition::col(prev_c.width);
                if target_pos < end_pos {
                    self.data.remove(&prev_pos);
                }
            }
            for i in 0..c.width {
                self.data.remove(&(target_pos + TerminalPosition::col(i)));
            }
            self.data.insert(target_pos, c);
        }
    }

    /// Returns the cell at `position`, or `None` if that position is covered by the
    /// continuation of a wide character that starts at an earlier column.
    ///
    /// A blank cell is returned for positions that have never been written.
    pub(crate) fn get_char(&self, position: TerminalPosition) -> Option<TerminalChar> {
        if let Some(ch) = self.data.get(&position).copied() {
            Some(ch)
        } else if let Some((pos, prev)) = self.data.range(..position).next_back()
            && position.row == pos.row
            && position.col < pos.col + prev.width
        {
            None
        } else {
            Some(TerminalChar::BLANK)
        }
    }

    /// Iterates over every cell position in row-major order (top-left first), yielding
    /// the position and the cell. Wide characters are yielded only at their starting
    /// column; the continuation columns of a wide character are skipped.
    pub fn chars(&self) -> impl '_ + Iterator<Item = (TerminalPosition, TerminalChar)> {
        let mut next_pos = TerminalPosition::ZERO;
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| TerminalPosition::row_col(row, col)))
            .filter_map(move |pos| {
                if pos < next_pos {
                    return None;
                }
                next_pos = pos;
                if let Some(c) = self.data.get(&pos).copied() {
                    next_pos.col += c.width;
                    Some((pos, c))
                } else {
                    next_pos.col += 1;
                    Some((pos, TerminalChar::BLANK))
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::BTreeMap};

    use super::*;

    const WIDE_CHARS: &[char] = &['\u{3042}', '\u{754c}', '\u{65e5}'];
    const ZERO_WIDTH_CHARS: &[char] = &['\u{301}', '\u{20dd}'];

    fn char_width(c: char) -> usize {
        if c.is_control() {
            0
        } else if WIDE_CHARS.contains(&c) {
            2
        } else {
            1
        }
    }

    /// Pushes a string of text onto the frame, handling newlines and per-character widths.
    fn push_text(frame: &mut TerminalFrame, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => frame.push_newline(),
                _ => {
                    let width = char_width(c);
                    if width > 0 {
                        frame.push_char(c, width, TerminalStyle::new());
                    }
                }
            }
        }
    }

    fn sample_pbt_size(ctx: &mut noprop::TestCaseContext) -> TerminalSize {
        TerminalSize::rows_cols(
            noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=12)
            }),
            noprop::sample_with_boundaries(ctx, &[0usize, 12], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=12)
            }),
        )
    }

    fn sample_pbt_text(ctx: &mut noprop::TestCaseContext) -> String {
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

    #[test]
    fn cursor_advances_by_width() {
        let size = TerminalSize::rows_cols(2, 4);
        let mut frame = TerminalFrame::new(size);
        frame.push_char('a', 1, TerminalStyle::new());
        frame.push_char('b', 1, TerminalStyle::new());
        frame.push_char('\u{3042}', 2, TerminalStyle::new());
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 4));
        frame.push_newline();
        assert_eq!(frame.cursor(), TerminalPosition::row_col(1, 0));
    }

    #[test]
    fn push_tab_advances_to_tab_stop() {
        let size = TerminalSize::rows_cols(2, 32);
        let mut frame = TerminalFrame::new(size);

        // From column 1, advance to the next stop (8).
        frame.push_char('a', 1, TerminalStyle::new());
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 8));

        // Already at a stop: advance one full stop.
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 16));

        // A non-aligned column advances to the next stop.
        frame.push_char('b', 1, TerminalStyle::new()); // col 17
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 24));
    }

    #[test]
    fn wide_char_continuation_is_blank_or_skipped() {
        let size = TerminalSize::rows_cols(1, 4);
        let mut frame = TerminalFrame::new(size);
        frame.push_char('\u{3042}', 2, TerminalStyle::new());
        frame.push_char('x', 1, TerminalStyle::new());

        assert_eq!(
            frame
                .get_char(TerminalPosition::row_col(0, 0))
                .map(|c| c.value),
            Some('\u{3042}')
        );
        assert_eq!(frame.get_char(TerminalPosition::row_col(0, 1)), None);
        assert_eq!(
            frame
                .get_char(TerminalPosition::row_col(0, 2))
                .map(|c| c.value),
            Some('x')
        );

        let positions: Vec<_> = frame.chars().map(|(p, c)| (p, c.value)).collect();
        assert_eq!(
            positions,
            [
                (TerminalPosition::row_col(0, 0), '\u{3042}'),
                (TerminalPosition::row_col(0, 2), 'x'),
                (TerminalPosition::row_col(0, 3), ' '),
            ]
        );
    }

    #[test]
    fn clips_cells_at_right_edge() {
        let size = TerminalSize::rows_cols(1, 3);
        let mut frame = TerminalFrame::new(size);
        frame.push_char('a', 1, TerminalStyle::new());
        frame.push_char('b', 1, TerminalStyle::new());
        frame.push_char('c', 1, TerminalStyle::new());
        // The row is full; the next cell is clipped but the cursor still advances.
        frame.push_char('d', 1, TerminalStyle::new());

        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 4));
        let stored: Vec<_> = frame
            .chars()
            .filter(|(_, c)| *c != TerminalChar::BLANK)
            .map(|(_, c)| c.value)
            .collect();
        assert_eq!(stored, ['a', 'b', 'c']);
    }

    #[test]
    fn draw_removes_partial_overlap_and_clips() {
        let size = TerminalSize::rows_cols(1, 4);
        let mut dest = TerminalFrame::new(size);
        dest.push_char('\u{3042}', 2, TerminalStyle::new()); // wide char at col 0-1
        dest.push_char('y', 1, TerminalStyle::new());

        // A one-cell source drawn over the continuation column of the wide char.
        let mut src = TerminalFrame::new(TerminalSize::rows_cols(1, 1));
        src.push_char('x', 1, TerminalStyle::new());

        // Draw 'x' over column 1, which is the continuation of the wide char.
        dest.draw(TerminalPosition::row_col(0, 1), &src);

        // The partially overlapped wide char should be removed entirely,
        // while the unaffected 'y' at column 2 remains.
        let stored: Vec<_> = dest
            .chars()
            .filter(|(_, c)| *c != TerminalChar::BLANK)
            .map(|(_, c)| c.value)
            .collect();
        assert_eq!(stored, ['x', 'y']);
    }

    /// `TerminalFrame::draw` must match a model that replays the overlap handling: a
    /// partially overlapped character is removed, the cells covered by the drawn
    /// character are cleared, and characters drawn outside the frame are ignored.
    #[test]
    fn pbt_draw_matches_model() -> noprop::TestResult {
        let observed_overlap = Cell::new(false);
        let observed_clipped = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            // Half of the cases force a partial overlap structurally: a wide character
            // whose second cell is overwritten by a drawn character.
            let structured = noprop::sample_bool(ctx);
            let (size, dest_text, src_text, position) = if structured {
                (
                    TerminalSize::rows_cols(1, 4),
                    "\u{3042}".to_string(),
                    "x".to_string(),
                    TerminalPosition::row_col(0, 1),
                )
            } else {
                (
                    sample_pbt_size(ctx),
                    sample_pbt_text(ctx),
                    sample_pbt_text(ctx),
                    TerminalPosition::row_col(
                        noprop::sample_usize_in(ctx, 0..=16),
                        noprop::sample_usize_in(ctx, 0..=16),
                    ),
                )
            };

            let mut dest = TerminalFrame::new(size);
            push_text(&mut dest, &dest_text);
            let mut src = TerminalFrame::new(size);
            push_text(&mut src, &src_text);

            let mut expected: BTreeMap<_, _> = dest
                .chars()
                .filter(|(_, c)| *c != TerminalChar::BLANK)
                .collect();
            let mut removals = 0usize;
            let mut skipped = 0usize;
            for (src_pos, c) in src.chars() {
                let target_pos = position + src_pos;
                if !size.contains(target_pos) {
                    skipped += 1;
                    continue;
                }
                if let Some((&prev_pos, prev_c)) = expected.range(..target_pos).next_back() {
                    let end_pos = prev_pos + TerminalPosition::col(prev_c.width);
                    if target_pos < end_pos {
                        expected.remove(&prev_pos);
                        removals += 1;
                    }
                }
                for i in 0..c.width {
                    expected.remove(&(target_pos + TerminalPosition::col(i)));
                }
                expected.insert(target_pos, c);
            }

            dest.draw(position, &src);
            let actual: BTreeMap<_, _> = dest
                .chars()
                .filter(|(_, c)| *c != TerminalChar::BLANK)
                .collect();
            let expected: BTreeMap<_, _> = expected
                .into_iter()
                .filter(|(_, c)| *c != TerminalChar::BLANK)
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
}
