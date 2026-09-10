use std::collections::BTreeMap;
use std::io::Write;

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

/// A single styled character in a [`TerminalFrame`].
///
/// The number of grid columns a character occupies, its [`width()`](Self::width), is
/// always `1` or more. A character wider than one column spans several adjacent
/// columns; the frame stores it only at its starting column.
///
/// Zero-width (combining) characters are not supported, and neither are control
/// characters; [`TerminalChar::new()`] rejects both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalChar {
    /// The character itself.
    value: char,

    /// The number of terminal columns this character occupies (`1` or more).
    width: usize,

    /// The style applied to this character.
    style: TerminalStyle,
}

impl TerminalChar {
    /// A blank character (a single space with no styling), used for unwritten positions.
    ///
    /// This is the sentinel returned for unwritten positions by [`TerminalFrame::chars()`].
    /// Pushing it is the same as writing a plain space, so [`TerminalFrame::push_char()`] does
    /// not treat it specially.
    pub const BLANK: Self = Self {
        value: ' ',
        width: 1,
        style: TerminalStyle::new(),
    };

    /// Makes a new styled character with the given width.
    ///
    /// Returns `None` when the character cannot be represented in a frame: if `value` is a
    /// control character, or `width` is `0`. Control characters are written with the
    /// dedicated methods ([`TerminalFrame::push_newline()`], [`TerminalFrame::push_tab()`]),
    /// and a zero-width character would occupy no column.
    pub const fn new(value: char, width: usize, style: TerminalStyle) -> Option<Self> {
        if value.is_control() || width == 0 {
            None
        } else {
            Some(Self {
                value,
                width,
                style,
            })
        }
    }

    /// The character itself.
    pub const fn value(self) -> char {
        self.value
    }

    /// The number of terminal columns this character occupies.
    pub const fn width(self) -> usize {
        self.width
    }

    /// The style applied to this character.
    pub const fn style(self) -> TerminalStyle {
        self.style
    }

    /// Returns `true` if this is the blank character used for unwritten positions.
    ///
    /// A blank character is a single space with no styling, equal to
    /// [`TerminalChar::BLANK`]. Use this to filter out unwritten positions when
    /// iterating with [`TerminalFrame::chars()`].
    pub fn is_blank(self) -> bool {
        self == Self::BLANK
    }
}

/// A frame buffer representing the terminal display state.
///
/// [`TerminalFrame`] is a buffer of styled characters. Each character stores the
/// glyph, the number of terminal columns it occupies, and the style.
///
/// Characters are written with [`push_char()`](Self::push_char) and advanced sequentially
/// from an internal cursor. Use [`push_newline()`](Self::push_newline) to move to the
/// next line and [`push_tab()`](Self::push_tab) to advance to a tab stop. A frame can be
/// composed onto another with [`draw()`](Self::draw), and its contents inspected with
/// [`chars()`](Self::chars).
///
/// # Examples
///
/// ```
/// let size = tuinix::TerminalSize::rows_cols(24, 80);
/// let mut frame = tuinix::TerminalFrame::new(size);
///
/// let bold = tuinix::TerminalStyle::new().bold();
/// frame.push_char(tuinix::TerminalChar::new('H', 1, bold).expect("valid char"));
/// frame.push_char(tuinix::TerminalChar::new('i', 1, bold).expect("valid char"));
/// frame.push_char(tuinix::TerminalChar::new('!', 1, bold).expect("valid char"));
/// frame.push_newline();
///
/// // A full-width (CJK) character occupies two columns.
/// frame.push_char(tuinix::TerminalChar::new('\u{3042}', 2, tuinix::TerminalStyle::new()).expect("valid character"));
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

    /// Returns the current cursor position (where the next character would be written).
    pub fn cursor(&self) -> TerminalPosition {
        self.tail
    }

    /// Writes a single styled character at the current cursor and advances the cursor.
    ///
    /// Returns `true` when the character was stored, and `false` when it was clipped
    /// because it did not fit within the frame: the character would extend past the right
    /// edge of the current row, or there was no row left beneath the cursor. Clipped
    /// characters are not stored, but the cursor still advances by the character's width,
    /// so a caller that wants to wrap the line does so itself.
    pub fn push_char(&mut self, ch: TerminalChar) -> bool {
        if self.tail.row < self.size.rows && self.tail.col + ch.width <= self.size.cols {
            self.data.insert(self.tail, ch);
            self.tail.col += ch.width;
            true
        } else {
            self.tail.col += ch.width;
            false
        }
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
    /// written explicitly (as with [`push_newline()`](Self::push_newline), existing content
    /// is neither cleared nor shifted).
    ///
    /// As with [`push_char()`](Self::push_char), the cursor may be advanced past the right
    /// edge of the frame; use [`push_newline()`](Self::push_newline) to wrap.
    ///
    /// # Panics
    ///
    /// Panics if `tab_width` is `0`.
    pub fn push_tab(&mut self, tab_width: usize) {
        assert!(tab_width > 0, "tab_width must be greater than zero");
        let col = self.tail.col;
        // Distance from `col` to the next tab stop. When `col` is already sitting on a
        // stop this is 0, so the `if` below moves to the *following* stop instead: a tab
        // always advances by at least one full stop and never lands on the current column.
        self.tail.col += (tab_width - col % tab_width) % tab_width;
        if self.tail.col == col {
            self.tail.col += tab_width;
        }
    }

    /// Draws the contents of another frame onto this one at the given position.
    ///
    /// The source frame is pasted as a rectangle: every cell position in the source is
    /// written to the corresponding position in this frame, including unwritten cells,
    /// which are pasted as a plain blank character ([`TerminalChar::BLANK`]) and therefore
    /// overwrite whatever was in the destination at that position.
    ///
    /// Characters that fall outside this frame, or that would extend past the right edge
    /// of a row, are ignored. A character that partially overlaps a wide character causes
    /// that wide character to be removed, so none of its columns are left behind as a
    /// partial glyph.
    pub fn draw(&mut self, position: TerminalPosition, frame: &TerminalFrame) {
        for (src_pos, c) in frame.chars() {
            let target_pos = position + src_pos;
            if target_pos.row >= self.size.rows || target_pos.col + c.width > self.size.cols {
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

    /// Returns the character at `position`, or `None` if that position is covered by the
    /// continuation of a wide character that starts at an earlier column.
    ///
    /// A blank character is returned for positions that have never been written.
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
    /// the position and the character. Wide characters are yielded only at their starting
    /// column; the continuation columns of a wide character are skipped.
    ///
    /// Unwritten positions are yielded as [`TerminalChar::BLANK`]. Use
    /// [`TerminalChar::is_blank()`] to visit only the characters that were written:
    ///
    /// ```
    /// let mut frame = tuinix::TerminalFrame::new(tuinix::TerminalSize::rows_cols(2, 4));
    /// frame.push_char(tuinix::TerminalChar::new('a', 1, Default::default()).expect("valid char"));
    ///
    /// let written = frame.chars().filter(|(_, c)| !c.is_blank()).count();
    /// assert_eq!(written, 1);
    /// ```
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

    /// Renders the difference between this frame and `prev` into a byte buffer.
    ///
    /// `prev` is the frame that was previously rendered to the terminal: the
    /// produced bytes contain the characters whose position, value, or style
    /// changed since `prev`. When `prev`'s size differs from this frame's size, or
    /// when `prev` is `None` (the first frame), every character is written.
    ///
    /// Unwritten positions are rendered as [`TerminalChar::BLANK`], so a frame is
    /// painted as a whole rectangle rather than as the characters that happen to
    /// be stored in it.
    ///
    /// `cursor` is the position where the terminal cursor is shown, or `None` to
    /// hide it.
    ///
    /// The returned bytes always start by hiding the cursor. They are not written
    /// or flushed: the caller is responsible for writing them to the driver and
    /// flushing it.
    ///
    /// # Examples
    ///
    /// ```
    /// let size = tuinix::TerminalSize::rows_cols(24, 80);
    /// let mut frame = tuinix::TerminalFrame::new(size);
    /// frame.push_char(tuinix::TerminalChar::new(
    ///     'h',
    ///     1,
    ///     tuinix::TerminalStyle::new(),
    /// ).expect("valid char"));
    ///
    /// let out = frame.render(None, None);
    /// ```
    pub fn render(
        &self,
        prev: Option<&TerminalFrame>,
        cursor: Option<TerminalPosition>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let _ = write!(out, "\x1b[?25l"); // hide cursor

        let resized = prev.is_none_or(|p| p.size() != self.size());
        let mut skipped = false;
        let mut last_style = None;
        let mut last_row = usize::MAX;
        for (position, c) in self.chars() {
            let old = prev.and_then(|p| p.get_char(position));
            if !resized && Some(c) == old {
                skipped = true;
                continue;
            }

            if skipped || last_row != position.row {
                let _ = write!(out, "\x1b[{};{}H", position.row + 1, position.col + 1);
            }
            if Some(c.style()) != last_style {
                let _ = write!(out, "{}", c.style());
            }
            let _ = write!(out, "{}", c.value());

            last_style = Some(c.style());
            last_row = position.row;
            skipped = false;
        }

        if let Some(position) = cursor {
            let _ = write!(out, "\x1b[{};{}H", position.row + 1, position.col + 1);
            let _ = write!(out, "\x1b[?25h"); // show cursor
        }

        out
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
        } else if ZERO_WIDTH_CHARS.contains(&c) {
            0
        } else {
            1
        }
    }

    /// Builds a valid character for the tests.
    fn ch(value: char, width: usize) -> TerminalChar {
        TerminalChar::new(value, width, TerminalStyle::new()).expect("valid char")
    }

    /// Pushes a string of text onto the frame, handling newlines and per-character widths.
    fn push_text(frame: &mut TerminalFrame, text: &str) {
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
        frame.push_char(ch('a', 1));
        frame.push_char(ch('b', 1));
        frame.push_char(ch('\u{3042}', 2));
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 4));
        frame.push_newline();
        assert_eq!(frame.cursor(), TerminalPosition::row_col(1, 0));
    }

    #[test]
    fn push_tab_advances_to_tab_stop() {
        let size = TerminalSize::rows_cols(2, 32);
        let mut frame = TerminalFrame::new(size);

        // From column 1, advance to the next stop (8).
        frame.push_char(ch('a', 1));
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 8));

        // Already at a stop: advance one full stop.
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 16));

        // A non-aligned column advances to the next stop.
        frame.push_char(ch('b', 1)); // col 17
        frame.push_tab(8);
        assert_eq!(frame.cursor(), TerminalPosition::row_col(0, 24));
    }

    #[test]
    fn wide_char_continuation_is_blank_or_skipped() {
        let size = TerminalSize::rows_cols(1, 4);
        let mut frame = TerminalFrame::new(size);
        frame.push_char(ch('\u{3042}', 2));
        frame.push_char(ch('x', 1));

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
        assert!(frame.push_char(ch('a', 1)));
        assert!(frame.push_char(ch('b', 1)));
        assert!(frame.push_char(ch('c', 1)));
        // The row is full; the next cell is clipped but the cursor still advances.
        assert!(!frame.push_char(ch('d', 1)));

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
        dest.push_char(ch('\u{3042}', 2)); // wide char at col 0-1
        dest.push_char(ch('y', 1));

        // A one-cell source drawn over the continuation column of the wide char.
        let mut src = TerminalFrame::new(TerminalSize::rows_cols(1, 1));
        src.push_char(ch('x', 1));

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
                if target_pos.row >= size.rows || target_pos.col + c.width > size.cols {
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

    /// Replays the byte stream produced by [`TerminalFrame::render()`] onto a model
    /// screen.
    ///
    /// It understands cursor positioning (`ESC [ <row> ; <col> H`), cursor visibility
    /// (`ESC [ ? 25 h` / `l`), and SGR style sequences (`ESC [ ... m`), which are
    /// ignored. Any other character is written at the cursor, overwriting the cells it
    /// covers and advancing the cursor by its width, the way a terminal does.
    #[derive(Debug)]
    struct ScreenModel {
        cells: BTreeMap<TerminalPosition, char>,
        cursor: TerminalPosition,
        writes: usize,
    }

    impl ScreenModel {
        fn new() -> Self {
            Self {
                cells: BTreeMap::new(),
                cursor: TerminalPosition::ZERO,
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
                            self.cursor = TerminalPosition::row_col(
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
                    && self.cursor < pos + TerminalPosition::col(char_width(*prev))
                {
                    self.cells.remove(&pos);
                }
                for i in 0..width {
                    self.cells.remove(&(self.cursor + TerminalPosition::col(i)));
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
        fn visible_cells(&self, size: TerminalSize) -> BTreeMap<TerminalPosition, char> {
            self.cells
                .iter()
                .filter(|(pos, _)| pos.row < size.rows && pos.col < size.cols)
                .map(|(pos, c)| (*pos, *c))
                .collect()
        }
    }

    /// The characters a frame paints, including the blanks of unwritten positions.
    fn rendered_cells(frame: &TerminalFrame) -> BTreeMap<TerminalPosition, char> {
        frame.chars().map(|(pos, c)| (pos, c.value())).collect()
    }

    /// Rendering without a previous frame must paint every cell of the frame, blanks
    /// included, so replaying the output reproduces `chars()`.
    #[test]
    fn pbt_render_full_redraw_matches_model() -> noprop::TestResult {
        const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
        let observed_wide = Cell::new(false);
        let observed_blank = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let size = sample_pbt_size(ctx);
            let mut frame = TerminalFrame::new(size);
            push_text(&mut frame, &sample_pbt_text(ctx));

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

    /// Rendering against a previous frame must leave the terminal showing this frame:
    /// replaying the difference onto the screen the previous frame painted reproduces
    /// `chars()`, and a size change forces a full redraw.
    #[test]
    fn pbt_render_diff_matches_model() -> noprop::TestResult {
        const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
        let observed_skip = Cell::new(false);
        let observed_resize = Cell::new(false);
        let observed_cursor = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let size = sample_pbt_size(ctx);
            // Half of the cases keep the size unchanged so that the differential path
            // is exercised; the rest may resize.
            let prev_size = if noprop::sample_bool(ctx) {
                size
            } else {
                sample_pbt_size(ctx)
            };
            let mut prev = TerminalFrame::new(prev_size);
            push_text(&mut prev, &sample_pbt_text(ctx));

            let mut frame = TerminalFrame::new(size);
            push_text(&mut frame, &sample_pbt_text(ctx));

            let cursor = if !size.is_empty() && noprop::sample_bool(ctx) {
                observed_cursor.set(true);
                Some(TerminalPosition::row_col(
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
}
