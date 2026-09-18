use std::collections::BTreeMap;
use std::io::Write;

use crate::{Position, Size, Style};

/// A single styled character in a [`Frame`].
///
/// The number of terminal columns a character occupies, its [`width()`](Self::width),
/// is always `1` or more. A character wider than one column spans several adjacent
/// columns; the frame stores it only at its starting column.
///
/// The width is declared by the caller and is the only source of that information: tuinix
/// does not measure how a character will actually be drawn, so the declared width is
/// trusted as-is and determines the layout. Deciding what width a character has is
/// therefore the caller's responsibility, as is keeping that decision consistent for a
/// given character.
///
/// Zero-width (combining) characters are not supported, and neither are control
/// characters; [`Char::new()`] rejects both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Char {
    /// The character itself.
    value: char,

    /// The number of terminal columns this character occupies (`1` or more).
    width: usize,

    /// The style applied to this character.
    style: Style,
}

impl Char {
    /// A blank character (a single space with no styling), used for unwritten positions.
    ///
    /// This is the sentinel returned for unwritten positions by [`Frame::chars()`].
    /// Writing it is the same as writing a plain space, so [`Frame::put_char()`] does
    /// not treat it specially.
    pub const BLANK: Self = Self {
        value: ' ',
        width: 1,
        style: Style::new(),
    };

    /// Makes a new styled character with the given width.
    ///
    /// Returns `None` when the character cannot be represented in a frame: if `value` is a
    /// control character, or `width` is `0`. Control characters are not stored in a
    /// frame at all; a newline or a tab is a move of the position the caller keeps
    /// ([`Position::next_line()`], [`Position::next_tab_stop()`]), and a zero-width
    /// character would occupy no column.
    ///
    /// Any `width` of `1` or more is accepted as given. tuinix does not measure how wide a
    /// character will actually be drawn, so the declared `width` is what the frame lays out
    /// with; supplying a width that does not match the character is a caller error that
    /// tuinix cannot detect.
    pub const fn new(value: char, width: usize, style: Style) -> Option<Self> {
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
    ///
    /// This is the width declared at [`Char::new()`], not a value measured from the
    /// character.
    pub const fn width(self) -> usize {
        self.width
    }

    /// The style applied to this character.
    pub const fn style(self) -> Style {
        self.style
    }

    /// Returns `true` if this is the blank character used for unwritten positions.
    ///
    /// A blank character is a single space with no styling, equal to
    /// [`Char::BLANK`]. Use this to filter out unwritten positions when
    /// iterating with [`Frame::chars()`].
    pub fn is_blank(self) -> bool {
        self == Self::BLANK
    }
}

/// A frame buffer representing the terminal display state.
///
/// [`Frame`] is a buffer of styled characters. Each character stores the
/// glyph, the number of terminal columns it occupies, and the style.
///
/// Every write names the position it targets, and nothing is carried over from
/// the write before it: [`put_char()`](Self::put_char) places one character and
/// returns the position just past it, and [`put_frame()`](Self::put_frame)
/// places another frame as a rectangle. Neither moves anything else, so a
/// caller that sweeps the screen keeps the position in a local of its own,
/// moving it with [`Position::next_line()`] and
/// [`Position::next_tab_stop()`]. [`fits()`](Self::fits) reports whether a
/// character would be drawn at a position, and the frame's contents are read
/// back with [`chars()`](Self::chars).
///
/// # Examples
///
/// ```
/// let size = tuinix::Size { rows: 24, cols: 80 };
/// let mut frame = tuinix::Frame::new(size);
///
/// let bold = tuinix::Style::new().bold();
/// let mut at = tuinix::Position::ORIGIN;
/// for (c, wide) in [('H', false), ('i', false), ('!', false), ('\u{3042}', true)] {
///     let width = if wide { 2 } else { 1 };
///     let ch = tuinix::Char::new(c, width, bold).expect("valid char");
///     at = frame.put_char(at, ch);
/// }
///
/// assert_eq!(at, tuinix::Position { row: 0, col: 5 });
/// ```
#[derive(Debug, Default, Clone)]
pub struct Frame {
    size: Size,
    data: BTreeMap<Position, Char>,
}

impl Frame {
    /// Makes a new, empty frame with the given size.
    pub fn new(size: Size) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
        }
    }

    /// Returns the size of this frame.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Returns `true` if a character written at `at` would be drawn.
    ///
    /// This reports what [`put_char()`](Self::put_char) will paint, not what is
    /// permitted: a write is always valid, and one that does not fit simply draws
    /// nothing. The two ways that happens are a row that does not exist (the row
    /// is at or past `size().rows`) and columns past the right edge (the
    /// character ends past `size().cols`).
    ///
    /// The character is tested as the whole span of cells it occupies, so a
    /// width-2 character with only one cell left in the row does not fit. Use this
    /// to decide what to do about a clip before writing — wrap with
    /// [`Position::next_line()`], pad, or stop.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
    /// let ch = tuinix::Char::new('a', 1, tuinix::Style::new()).expect("valid char");
    /// assert!(frame.fits(tuinix::Position { row: 0, col: 3 }, ch));
    /// assert!(!frame.fits(tuinix::Position { row: 0, col: 4 }, ch));
    ///
    /// // A wide character needs both of its columns.
    /// let wide = tuinix::Char::new('\u{3042}', 2, tuinix::Style::new()).expect("valid char");
    /// assert!(frame.fits(tuinix::Position { row: 0, col: 2 }, wide));
    /// assert!(!frame.fits(tuinix::Position { row: 0, col: 3 }, wide));
    /// ```
    pub fn fits(&self, at: Position, ch: Char) -> bool {
        at.row < self.size.rows && at.col + ch.width <= self.size.cols
    }

    /// Writes a single styled character at `at` and returns the position just
    /// past it.
    ///
    /// The character is stored when it [fits](Self::fits), and draws nothing when
    /// it does not. Either way the returned position is `at.advance(ch.width())`,
    /// so a run of writes advances identically whether or not each character
    /// landed. A character that does not fit is a normal outcome for a caller
    /// sweeping a row, not a failure, which is why the return type carries no
    /// error or "did it fit" flag: ask [`fits()`](Self::fits) before writing to
    /// act on a clip.
    ///
    /// The position returned is the caller's to keep; the frame remembers nothing
    /// about it. A caller filling a line passes it straight back in:
    ///
    /// ```
    /// let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 3 });
    /// let mut at = tuinix::Position::ORIGIN;
    /// for c in "abcd".chars() {
    ///     let ch = tuinix::Char::new(c, 1, tuinix::Style::new()).expect("valid char");
    ///     if !frame.fits(at, ch) {
    ///         at = at.next_line();
    ///     }
    ///     at = frame.put_char(at, ch);
    /// }
    /// assert_eq!(at, tuinix::Position { row: 1, col: 1 });
    /// ```
    ///
    /// A character written where another one already is replaces it, as described
    /// on [`put_frame()`](Self::put_frame).
    pub fn put_char(&mut self, at: Position, ch: Char) -> Position {
        if self.fits(at, ch) {
            self.put_cell(at, ch);
        }
        at.advance(ch.width)
    }

    /// Stores `ch` at `at`, replacing whatever is there.
    ///
    /// The caller must have checked that the character fits.
    fn put_cell(&mut self, at: Position, ch: Char) {
        // A wide character starting to the left of `at` owns the cells `at`
        // covers, so it has to go whole: removing only the cells that are being
        // overwritten would leave its remaining columns behind as a partial
        // glyph.
        if let Some((&prev_pos, prev_c)) = self.data.range(..at).next_back() {
            let end_col = prev_pos.col + prev_c.width;
            if at.row == prev_pos.row && at.col < end_col {
                self.data.remove(&prev_pos);
            }
        }
        // Clear the cells this character covers, including the continuation
        // columns of a wide character that may be stored at one of them.
        for i in 0..ch.width {
            self.data.remove(&Position {
                row: at.row,
                col: at.col + i,
            });
        }
        self.data.insert(at, ch);
    }

    /// Writes the contents of `source` onto this frame as a rectangle, with the
    /// source's top-left corner at `at`.
    ///
    /// The source is pasted as a rectangle: every cell position in the source is
    /// written to the corresponding position in this frame, including unwritten cells,
    /// which are pasted as a plain blank character ([`Char::BLANK`]) and therefore
    /// overwrite whatever was in the destination at that position. In other words this
    /// replaces a rectangle; it does not merge one, and the source's blanks are not
    /// transparent.
    ///
    /// Characters that fall outside this frame, or that would extend past the right edge
    /// of a row, are ignored. A character that partially overlaps a wide character causes
    /// that wide character to be removed, so none of its columns are left behind as a
    /// partial glyph.
    ///
    /// This returns nothing, unlike [`put_char()`](Self::put_char): a rectangle of
    /// cells has no single "position just past it", and a caller placing one is not
    /// sweeping a run with a next write to aim at.
    pub fn put_frame(&mut self, at: Position, source: &Frame) {
        for (src_pos, c) in source.chars() {
            let target_pos = Position {
                row: at.row + src_pos.row,
                col: at.col + src_pos.col,
            };
            if self.fits(target_pos, c) {
                self.put_cell(target_pos, c);
            }
        }
    }

    /// Returns the character at `position`, or `None` if that position is covered by the
    /// continuation of a wide character that starts at an earlier column.
    ///
    /// A blank character is returned for positions that have never been written.
    pub(crate) fn get_char(&self, position: Position) -> Option<Char> {
        if let Some(ch) = self.data.get(&position).copied() {
            Some(ch)
        } else if let Some((pos, prev)) = self.data.range(..position).next_back()
            && position.row == pos.row
            && position.col < pos.col + prev.width
        {
            None
        } else {
            Some(Char::BLANK)
        }
    }

    /// Iterates over every cell position in row-major order (top-left first), yielding
    /// the position and the character. Wide characters are yielded only at their starting
    /// column; the continuation columns of a wide character are skipped.
    ///
    /// Unwritten positions are yielded as [`Char::BLANK`]. Use
    /// [`Char::is_blank()`] to visit only the characters that were written:
    ///
    /// ```
    /// let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
    /// frame.put_char(
    ///     tuinix::Position::ORIGIN,
    ///     tuinix::Char::new('a', 1, Default::default()).expect("valid char"),
    /// );
    ///
    /// let written = frame.chars().filter(|(_, c)| !c.is_blank()).count();
    /// assert_eq!(written, 1);
    /// ```
    pub fn chars(&self) -> impl '_ + Iterator<Item = (Position, Char)> {
        let mut next_pos = Position::ORIGIN;
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| Position { row, col }))
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
                    Some((pos, Char::BLANK))
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
    /// Unwritten positions are rendered as [`Char::BLANK`], so a frame is
    /// painted as a whole rectangle rather than as the characters that happen to
    /// be stored in it.
    ///
    /// `cursor` is the position where the terminal cursor is shown, or `None` to
    /// hide it.
    ///
    /// The returned bytes are not written or flushed: the caller is responsible
    /// for writing them to the driver and flushing it.
    ///
    /// # Examples
    ///
    /// ```
    /// let size = tuinix::Size { rows: 24, cols: 80 };
    /// let mut frame = tuinix::Frame::new(size);
    /// frame.put_char(
    ///     tuinix::Position::ORIGIN,
    ///     tuinix::Char::new('h', 1, tuinix::Style::new()).expect("valid char"),
    /// );
    ///
    /// let out = frame.render(None, None);
    /// ```
    pub fn render(&self, prev: Option<&Frame>, cursor: Option<Position>) -> Vec<u8> {
        let mut out = Vec::new();
        // Cursor visibility is not part of a frame, so `render` cannot know whether
        // the terminal is currently showing the cursor. Hiding is idempotent, so it
        // is emitted unconditionally: this makes `None` mean "hidden" without
        // comparing against `prev`, and stops a cursor that moved between frames
        // from flickering at its old position.
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
    use super::*;

    /// Builds a valid character for the tests.
    fn ch(value: char, width: usize) -> Char {
        Char::new(value, width, Style::new()).expect("valid char")
    }

    #[test]
    fn put_char_returns_the_next_position() {
        let size = Size { rows: 2, cols: 4 };
        let mut frame = Frame::new(size);
        let mut at = Position::ORIGIN;
        at = frame.put_char(at, ch('a', 1));
        at = frame.put_char(at, ch('b', 1));
        at = frame.put_char(at, ch('\u{3042}', 2));
        assert_eq!(at, Position { row: 0, col: 4 });
        assert_eq!(at.next_line(), Position { row: 1, col: 0 });
    }

    #[test]
    fn next_tab_stop_advances_to_a_tab_stop() {
        // From column 1, advance to the next stop (8).
        let at = Position { row: 3, col: 1 };
        assert_eq!(at.next_tab_stop(8), Position { row: 3, col: 8 });

        // Already at a stop: advance one full stop.
        let at = Position { row: 3, col: 8 };
        assert_eq!(at.next_tab_stop(8), Position { row: 3, col: 16 });

        // A non-aligned column advances to the next stop.
        let at = Position { row: 3, col: 17 };
        assert_eq!(at.next_tab_stop(8), Position { row: 3, col: 24 });
    }

    #[test]
    fn wide_char_continuation_is_blank_or_skipped() {
        let size = Size { rows: 1, cols: 4 };
        let mut frame = Frame::new(size);
        let at = frame.put_char(Position::ORIGIN, ch('\u{3042}', 2));
        frame.put_char(at, ch('x', 1));

        assert_eq!(
            frame.get_char(Position { row: 0, col: 0 }).map(|c| c.value),
            Some('\u{3042}')
        );
        assert_eq!(frame.get_char(Position { row: 0, col: 1 }), None);
        assert_eq!(
            frame.get_char(Position { row: 0, col: 2 }).map(|c| c.value),
            Some('x')
        );

        let positions: Vec<_> = frame.chars().map(|(p, c)| (p, c.value)).collect();
        assert_eq!(
            positions,
            [
                (Position { row: 0, col: 0 }, '\u{3042}'),
                (Position { row: 0, col: 2 }, 'x'),
                (Position { row: 0, col: 3 }, ' '),
            ]
        );
    }

    #[test]
    fn clips_cells_at_right_edge() {
        let size = Size { rows: 1, cols: 3 };
        let mut frame = Frame::new(size);
        let mut at = Position::ORIGIN;
        for c in ['a', 'b', 'c', 'd'] {
            // The row is full by the time 'd' arrives, so it is not stored; the
            // position still advances by its width either way.
            at = frame.put_char(at, ch(c, 1));
        }
        assert_eq!(at, Position { row: 0, col: 4 });

        let stored: Vec<_> = frame
            .chars()
            .filter(|(_, c)| *c != Char::BLANK)
            .map(|(_, c)| c.value)
            .collect();
        assert_eq!(stored, ['a', 'b', 'c']);
    }

    #[test]
    fn put_frame_removes_partial_overlap_and_clips() {
        let size = Size { rows: 1, cols: 4 };
        let mut dest = Frame::new(size);
        let at = dest.put_char(Position::ORIGIN, ch('\u{3042}', 2)); // wide char at col 0-1
        dest.put_char(at, ch('y', 1));

        // A one-cell source drawn over the continuation column of the wide char.
        let mut src = Frame::new(Size { rows: 1, cols: 1 });
        src.put_char(Position::ORIGIN, ch('x', 1));

        // Draw 'x' over column 1, which is the continuation of the wide char.
        dest.put_frame(Position { row: 0, col: 1 }, &src);

        // The partially overlapped wide char should be removed entirely,
        // while the unaffected 'y' at column 2 remains.
        let stored: Vec<_> = dest
            .chars()
            .filter(|(_, c)| *c != Char::BLANK)
            .map(|(_, c)| c.value)
            .collect();
        assert_eq!(stored, ['x', 'y']);
    }
}
