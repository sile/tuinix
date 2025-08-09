use std::{collections::BTreeMap, num::NonZeroUsize};

use crate::{TerminalPosition, TerminalSize, TerminalStyle};

/// A frame buffer representing the terminal display state.
///
/// [`TerminalFrame`] manages a collection of styled characters with their positions,
/// providing efficient drawing operations for terminal-based user interfaces.
/// It maintains character positions, styles, and widths to accurately represent
/// what will be displayed on the terminal.
///
/// This struct serves as the primary drawing surface for terminal UIs, allowing
/// you to:
/// - Write text with different styles using the `write!()` macro
/// - Compose multiple frames together
/// - Draw frames to the terminal using `Terminal::draw()`
///
/// # Writing to a Frame
///
/// [`TerminalFrame`] implements the [`std::fmt::Write`] trait, which allows using
/// the `write!()` and `writeln!()` macros to add content to the frame with styling.
///
/// # Drawing Frames
///
/// After creating and populating a [`TerminalFrame`], use [`Terminal::draw()`](crate::Terminal::draw) to
/// efficiently render the frame to the terminal screen. The terminal implementation
/// optimizes by only updating changed portions of the screen.
///
/// # Examples
///
/// ```
/// use std::fmt::Write;
/// use tuinix::{TerminalFrame, TerminalSize, TerminalStyle};
///
/// // Create a new frame with specified dimensions
/// let size = TerminalSize::rows_cols(24, 80);
/// let mut frame: TerminalFrame = TerminalFrame::new(size);
///
/// // Write text to the frame
/// writeln!(frame, "Hello, world!")?;
///
/// // Use styling
/// let bold = TerminalStyle::new().bold();
/// let reset = TerminalStyle::new();
/// writeln!(frame, "{bold}This text is bold{reset}")?;
///
/// // To render this frame to the terminal:
/// // terminal.draw(frame)?;
/// # Ok::<_, std::fmt::Error>(())
/// ```
#[derive(Debug, Default, Clone)]
pub struct TerminalFrame<W = FixedCharWidthEstimator> {
    size: TerminalSize,
    data: BTreeMap<TerminalPosition, TerminalChar>,
    tail: TerminalPosition,
    current_style: TerminalStyle,
    escape_sequence: String,
    char_width_estimator: W,
}

impl<W: Default> TerminalFrame<W> {
    /// Makes a new frame with the given size and default character width estimator.
    pub fn new(size: TerminalSize) -> Self {
        Self::with_char_width_estimator(size, W::default())
    }
}

impl<W> TerminalFrame<W> {
    /// Makes a new frame with the given size and char width estimator.
    pub fn with_char_width_estimator(size: TerminalSize, char_width_estimator: W) -> Self {
        Self {
            size,
            data: BTreeMap::new(),
            tail: TerminalPosition::ZERO,
            current_style: TerminalStyle::new(),
            escape_sequence: String::new(),
            char_width_estimator,
        }
    }

    /// Returns the size of this frame.
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Returns the current cursor position in the frame.
    ///
    /// This represents where the next character would be written when using
    /// `write!()` or `writeln!()` macros on this frame.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fmt::Write;
    /// use tuinix::{TerminalFrame, TerminalPosition, TerminalSize};
    ///
    /// let mut frame: TerminalFrame = TerminalFrame::new(TerminalSize::rows_cols(10, 20));
    /// write!(frame, "Hello")?;
    ///
    /// assert_eq!(frame.cursor().col, 5);
    /// # Ok::<(), std::fmt::Error>(())
    /// ```
    pub fn cursor(&self) -> TerminalPosition {
        self.tail
    }

    /// Draws the contents of another frame onto this frame at the specified position.
    ///
    /// This method copies all the characters from the source frame and positions them
    /// relative to the provided position on this frame. Characters that would fall outside
    /// the bounds of this frame are ignored.
    ///
    /// The method performs several important tasks:
    /// - Properly handles character collision and overlapping
    /// - Removes any characters that would be partially overlapped by wide characters
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fmt::Write;
    /// use tuinix::{TerminalFrame, TerminalPosition, TerminalSize};
    ///
    /// // Create a main frame
    /// let mut main_frame: TerminalFrame = TerminalFrame::new(TerminalSize::rows_cols(24, 80));
    ///
    /// // Create a smaller frame to be drawn onto the main frame
    /// let mut sub_frame: TerminalFrame = TerminalFrame::new(TerminalSize::rows_cols(5, 20));
    /// write!(sub_frame, "This is a sub-frame")?;
    ///
    /// // Draw the sub-frame at position (2, 10) on the main frame
    /// main_frame.draw(TerminalPosition::row_col(2, 10), &sub_frame);
    /// # Ok::<(), std::fmt::Error>(())
    /// ```
    pub fn draw<X>(&mut self, position: TerminalPosition, frame: &TerminalFrame<X>) {
        for (src_pos, c) in frame.chars() {
            let target_pos = position + src_pos;
            if !self.size.contains(target_pos) {
                continue;
            }

            if let Some((&prev_pos, prev_c)) = self.data.range(..target_pos).next_back() {
                let end_pos = prev_pos + TerminalPosition::col(prev_c.width.get());
                if target_pos < end_pos {
                    self.data.remove(&prev_pos);
                }
            }
            for i in 0..c.width.get() {
                self.data.remove(&(target_pos + TerminalPosition::col(i)));
            }
            self.data.insert(target_pos, c);
        }
    }

    pub(crate) fn get_char(&self, position: TerminalPosition) -> Option<TerminalChar> {
        if let Some(ch) = self.data.get(&position).copied() {
            // Character exists at this exact position - return it
            Some(ch)
        } else if let Some((pos, prev)) = self.data.range(..position).next_back()
            && position.row == pos.row
            && position.col < pos.col + prev.width.get()
        {
            // Position falls within a wide character's display area but not at its starting position.
            // Return None to indicate this position is occupied by a multi-column character
            // that starts at an earlier column.
            None
        } else {
            // No character at this position and it's not part of a wide character's display area.
            // Return a blank character to represent empty space.
            Some(TerminalChar::BLANK)
        }
    }

    pub(crate) fn chars(&self) -> impl '_ + Iterator<Item = (TerminalPosition, TerminalChar)> {
        let mut next_pos = TerminalPosition::ZERO;
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| TerminalPosition::row_col(row, col)))
            .filter_map(move |pos| {
                if pos < next_pos {
                    // Skip this position as it's part of a multi-column
                    // character's display space, but not the actual starting
                    // position of the character.
                    return None;
                }

                next_pos = pos;
                if let Some(c) = self.data.get(&pos).copied() {
                    next_pos.col += c.width.get();
                    Some((pos, c))
                } else {
                    next_pos.col += 1;
                    let c = TerminalChar::BLANK;
                    Some((pos, c))
                }
            })
    }

    pub(crate) fn finish(self) -> TerminalFrame<FixedCharWidthEstimator> {
        TerminalFrame {
            size: self.size,
            data: self.data,
            tail: self.tail,
            current_style: self.current_style,
            escape_sequence: self.escape_sequence,
            char_width_estimator: FixedCharWidthEstimator,
        }
    }
}

impl<W: EstimateCharWidth> std::fmt::Write for TerminalFrame<W> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for c in s.chars() {
            if !self.escape_sequence.is_empty() {
                self.escape_sequence.push(c);
                if c.is_ascii_alphabetic() {
                    self.current_style = self
                        .escape_sequence
                        .parse()
                        .expect("escape sequence should be generated via `TerminalStyle`");
                    self.escape_sequence.clear();
                }
                continue;
            } else if c == '\x1b' {
                self.escape_sequence.push(c);
                continue;
            } else if c == '\n' {
                self.tail.row += 1;
                self.tail.col = 0;
                continue;
            }

            let Some(width) = NonZeroUsize::new(self.char_width_estimator.estimate_char_width(c))
            else {
                continue;
            };

            if self.tail.row < self.size.rows && self.tail.col + width.get() <= self.size.cols {
                self.data.insert(
                    self.tail,
                    TerminalChar {
                        style: self.current_style,
                        width,
                        value: c,
                    },
                );
            }
            self.tail.col += width.get();
        }

        Ok(())
    }
}

/// Trait for estimating the display width of characters in a terminal.
///
/// This trait provides a way to determine how much horizontal space a character
/// will occupy when rendered in a terminal.
///
/// # Limitations
///
/// - Tab characters (`\t`): The width of a tab depends on the current cursor position
///   and tab stop settings, not just the character itself. Since this trait only
///   takes a single character as input without position context, it cannot
///   accurately determine the visual width of tab characters.
/// - Zero-width combining characters: Characters like accents and diacritical marks
///   that modify previous characters (e.g., `é` can be represented as `e` followed
///   by the combining acute accent `\u{0301}`) have no width on their own but change
///   the appearance of preceding characters. The current interface cannot properly
///   handle these because it examines each character in isolation without
///   considering adjacent characters.
pub trait EstimateCharWidth {
    /// Estimates the display width of a character.
    ///
    /// Returns the number of columns the character will occupy in the terminal.
    fn estimate_char_width(&self, c: char) -> usize;
}

/// A character width estimator that assumes most characters have a fixed width of 1 column.
///
/// This simple implementation of [`EstimateCharWidth`] assigns:
/// - Width of 0 to all control characters (they don't take visual space)
/// - Width of 1 to all other characters
///
/// # Limitations
///
/// This estimator doesn't correctly handle:
/// - Wide characters like CJK (Chinese, Japanese, Korean) that take 2 columns
/// - Emojis and other complex Unicode characters
///
/// For better support of these characters, consider implementing a more
/// sophisticated width estimator based on Unicode width calculation libraries.
#[derive(Debug, Default, Clone)]
pub struct FixedCharWidthEstimator;

impl EstimateCharWidth for FixedCharWidthEstimator {
    fn estimate_char_width(&self, c: char) -> usize {
        if c.is_control() { 0 } else { 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalChar {
    pub style: TerminalStyle,
    pub width: NonZeroUsize,
    pub value: char,
}

impl TerminalChar {
    const BLANK: Self = Self {
        style: TerminalStyle::new(),
        width: NonZeroUsize::MIN,
        value: ' ',
    };
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use unicode_width::UnicodeWidthChar;

    use super::*;

    struct UnicodeCharWidthEstimator;

    impl EstimateCharWidth for UnicodeCharWidthEstimator {
        fn estimate_char_width(&self, c: char) -> usize {
            c.width().unwrap_or_default()
        }
    }

    #[test]
    fn unicode_char_width() {
        let size = TerminalSize::rows_cols(10, 20);
        let mut frame = TerminalFrame::with_char_width_estimator(size, UnicodeCharWidthEstimator);

        // Write Japanese characters "おはよう" (good morning)
        write!(frame, "おはよう").unwrap();

        // Check the cursor position - each character should take 2 columns
        assert_eq!(frame.cursor().col, 8); // 4 characters × 2 columns each = 8

        // Verify each character is stored correctly with proper width
        let chars: Vec<_> = frame.chars().filter(|(_, c)| c.value != ' ').collect();

        assert_eq!(chars.len(), 4);
        assert_eq!(chars[0].1.value, 'お');
        assert_eq!(chars[0].1.width.get(), 2);
        assert_eq!(chars[1].1.value, 'は');
        assert_eq!(chars[1].1.width.get(), 2);
        assert_eq!(chars[2].1.value, 'よ');
        assert_eq!(chars[2].1.width.get(), 2);
        assert_eq!(chars[3].1.value, 'う');
        assert_eq!(chars[3].1.width.get(), 2);

        // Check positions of each character
        assert_eq!(chars[0].0, TerminalPosition::row_col(0, 0));
        assert_eq!(chars[1].0, TerminalPosition::row_col(0, 2));
        assert_eq!(chars[2].0, TerminalPosition::row_col(0, 4));
        assert_eq!(chars[3].0, TerminalPosition::row_col(0, 6));
    }
}
