use std::collections::BTreeMap;
use std::io::Write;

use crate::{Position, Size, Style};

/// A single styled character in a [`Frame`].
///
/// The number of grid columns a character occupies, its [`width()`](Self::width), is
/// always `1` or more. A character wider than one column spans several adjacent
/// columns; the frame stores it only at its starting column.
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
    /// Pushing it is the same as writing a plain space, so [`Frame::push_char()`] does
    /// not treat it specially.
    pub const BLANK: Self = Self {
        value: ' ',
        width: 1,
        style: Style::new(),
    };

    /// Makes a new styled character with the given width.
    ///
    /// Returns `None` when the character cannot be represented in a frame: if `value` is a
    /// control character, or `width` is `0`. Control characters are written with the
    /// dedicated methods ([`Frame::push_newline()`], [`Frame::push_tab()`]),
    /// and a zero-width character would occupy no column.
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
/// Characters are written with [`push_char()`](Self::push_char) and advanced sequentially
/// from an internal cursor. Use [`push_newline()`](Self::push_newline) to move to the
/// next line and [`push_tab()`](Self::push_tab) to advance to a tab stop. A frame can be
/// composed onto another with [`draw()`](Self::draw), and its contents inspected with
/// [`chars()`](Self::chars).
///
/// # Examples
///
/// ```
/// let size = tuinix::Size::rows_cols(24, 80);
/// let mut frame = tuinix::Frame::new(size);
///
/// let bold = tuinix::Style::new().bold();
/// frame.push_char(tuinix::Char::new('H', 1, bold).expect("valid char"));
/// frame.push_char(tuinix::Char::new('i', 1, bold).expect("valid char"));
/// frame.push_char(tuinix::Char::new('!', 1, bold).expect("valid char"));
/// frame.push_newline();
///
/// // A full-width (CJK) character occupies two columns.
/// frame.push_char(tuinix::Char::new('\u{3042}', 2, tuinix::Style::new()).expect("valid character"));
///
/// assert_eq!(frame.cursor().col, 2);
/// ```
#[derive(Debug, Default, Clone)]
pub struct Frame {
    size: Size,
    data: BTreeMap<Position, Char>,
    tail: Position,
}

impl Frame {
    /// Makes a new, empty frame with the given size.
    pub fn new(size: Size) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
            tail: Position::ZERO,
        }
    }

    /// Returns the size of this frame.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Returns the current cursor position (where the next character would be written).
    pub fn cursor(&self) -> Position {
        self.tail
    }

    /// Writes a single styled character at the current cursor and advances the cursor.
    ///
    /// Returns `true` when the character was stored, and `false` when it was clipped
    /// because it did not fit within the frame: the character would extend past the right
    /// edge of the current row, or there was no row left beneath the cursor. Clipped
    /// characters are not stored, but the cursor still advances by the character's width,
    /// so a caller that wants to wrap the line does so itself.
    pub fn push_char(&mut self, ch: Char) -> bool {
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
    /// which are pasted as a plain blank character ([`Char::BLANK`]) and therefore
    /// overwrite whatever was in the destination at that position.
    ///
    /// Characters that fall outside this frame, or that would extend past the right edge
    /// of a row, are ignored. A character that partially overlaps a wide character causes
    /// that wide character to be removed, so none of its columns are left behind as a
    /// partial glyph.
    pub fn draw(&mut self, position: Position, frame: &Frame) {
        for (src_pos, c) in frame.chars() {
            let target_pos = position + src_pos;
            if target_pos.row >= self.size.rows || target_pos.col + c.width > self.size.cols {
                continue;
            }

            if let Some((&prev_pos, prev_c)) = self.data.range(..target_pos).next_back() {
                let end_pos = prev_pos + Position::col(prev_c.width);
                if target_pos < end_pos {
                    self.data.remove(&prev_pos);
                }
            }
            for i in 0..c.width {
                self.data.remove(&(target_pos + Position::col(i)));
            }
            self.data.insert(target_pos, c);
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
    /// let mut frame = tuinix::Frame::new(tuinix::Size::rows_cols(2, 4));
    /// frame.push_char(tuinix::Char::new('a', 1, Default::default()).expect("valid char"));
    ///
    /// let written = frame.chars().filter(|(_, c)| !c.is_blank()).count();
    /// assert_eq!(written, 1);
    /// ```
    pub fn chars(&self) -> impl '_ + Iterator<Item = (Position, Char)> {
        let mut next_pos = Position::ZERO;
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| Position::row_col(row, col)))
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
    /// let size = tuinix::Size::rows_cols(24, 80);
    /// let mut frame = tuinix::Frame::new(size);
    /// frame.push_char(tuinix::Char::new(
    ///     'h',
    ///     1,
    ///     tuinix::Style::new(),
    /// ).expect("valid char"));
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
    fn cursor_advances_by_width() {
        let size = Size::rows_cols(2, 4);
        let mut frame = Frame::new(size);
        frame.push_char(ch('a', 1));
        frame.push_char(ch('b', 1));
        frame.push_char(ch('\u{3042}', 2));
        assert_eq!(frame.cursor(), Position::row_col(0, 4));
        frame.push_newline();
        assert_eq!(frame.cursor(), Position::row_col(1, 0));
    }

    #[test]
    fn push_tab_advances_to_tab_stop() {
        let size = Size::rows_cols(2, 32);
        let mut frame = Frame::new(size);

        // From column 1, advance to the next stop (8).
        frame.push_char(ch('a', 1));
        frame.push_tab(8);
        assert_eq!(frame.cursor(), Position::row_col(0, 8));

        // Already at a stop: advance one full stop.
        frame.push_tab(8);
        assert_eq!(frame.cursor(), Position::row_col(0, 16));

        // A non-aligned column advances to the next stop.
        frame.push_char(ch('b', 1)); // col 17
        frame.push_tab(8);
        assert_eq!(frame.cursor(), Position::row_col(0, 24));
    }

    #[test]
    fn wide_char_continuation_is_blank_or_skipped() {
        let size = Size::rows_cols(1, 4);
        let mut frame = Frame::new(size);
        frame.push_char(ch('\u{3042}', 2));
        frame.push_char(ch('x', 1));

        assert_eq!(
            frame.get_char(Position::row_col(0, 0)).map(|c| c.value),
            Some('\u{3042}')
        );
        assert_eq!(frame.get_char(Position::row_col(0, 1)), None);
        assert_eq!(
            frame.get_char(Position::row_col(0, 2)).map(|c| c.value),
            Some('x')
        );

        let positions: Vec<_> = frame.chars().map(|(p, c)| (p, c.value)).collect();
        assert_eq!(
            positions,
            [
                (Position::row_col(0, 0), '\u{3042}'),
                (Position::row_col(0, 2), 'x'),
                (Position::row_col(0, 3), ' '),
            ]
        );
    }

    #[test]
    fn clips_cells_at_right_edge() {
        let size = Size::rows_cols(1, 3);
        let mut frame = Frame::new(size);
        assert!(frame.push_char(ch('a', 1)));
        assert!(frame.push_char(ch('b', 1)));
        assert!(frame.push_char(ch('c', 1)));
        // The row is full; the next cell is clipped but the cursor still advances.
        assert!(!frame.push_char(ch('d', 1)));

        assert_eq!(frame.cursor(), Position::row_col(0, 4));
        let stored: Vec<_> = frame
            .chars()
            .filter(|(_, c)| *c != Char::BLANK)
            .map(|(_, c)| c.value)
            .collect();
        assert_eq!(stored, ['a', 'b', 'c']);
    }

    #[test]
    fn draw_removes_partial_overlap_and_clips() {
        let size = Size::rows_cols(1, 4);
        let mut dest = Frame::new(size);
        dest.push_char(ch('\u{3042}', 2)); // wide char at col 0-1
        dest.push_char(ch('y', 1));

        // A one-cell source drawn over the continuation column of the wide char.
        let mut src = Frame::new(Size::rows_cols(1, 1));
        src.push_char(ch('x', 1));

        // Draw 'x' over column 1, which is the continuation of the wide char.
        dest.draw(Position::row_col(0, 1), &src);

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
