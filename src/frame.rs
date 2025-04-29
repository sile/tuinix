use std::collections::BTreeMap;

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
/// let size = TerminalSize { rows: 24, cols: 80 };
/// let mut frame = TerminalFrame::new(size);
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
    /// Makes a new frame with the given size and a default char width estimator.
    pub fn new(size: TerminalSize) -> Self {
        Self::with_char_width_measurer(size, W::default())
    }
}

impl<W> TerminalFrame<W> {
    /// Makes a new frame with the given size and char width estimator.
    pub fn with_char_width_measurer(size: TerminalSize, char_width_estimator: W) -> Self {
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
    /// let mut frame = TerminalFrame::new(TerminalSize { rows: 10, cols: 20 });
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
    /// let mut main_frame = TerminalFrame::new(TerminalSize { rows: 24, cols: 80 });
    ///
    /// // Create a smaller frame to be drawn onto the main frame
    /// let mut sub_frame = TerminalFrame::new(TerminalSize { rows: 5, cols: 20 });
    /// write!(sub_frame, "This is a sub-frame")?;
    ///
    /// // Draw the sub-frame at position (2, 10) on the main frame
    /// main_frame.draw(TerminalPosition::row_col(2, 10), &sub_frame);
    /// # Ok::<(), std::fmt::Error>(())
    /// ```
    pub fn draw(&mut self, position: TerminalPosition, frame: &Self) {
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

    pub(crate) fn chars(&self) -> impl '_ + Iterator<Item = (TerminalPosition, TerminalChar)> {
        let mut last_style = TerminalStyle::new();
        (0..self.size.rows)
            .flat_map(|row| (0..self.size.cols).map(move |col| TerminalPosition::row_col(row, col)))
            .map(move |pos| {
                if let Some(c) = self.data.get(&pos).copied() {
                    last_style = c.style;
                    (pos, c)
                } else {
                    if pos >= self.tail {
                        last_style = self.current_style;
                    }
                    let c = TerminalChar {
                        style: last_style,
                        width: 1,
                        value: ' ',
                    };
                    (pos, c)
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

            let width = self.char_width_estimator.estimate_char_width(c);
            if width == 0 {
                continue;
            }

            if self.tail.row < self.size.rows && self.tail.col + width <= self.size.cols {
                self.data.insert(
                    self.tail,
                    TerminalChar {
                        style: self.current_style,
                        width,
                        value: c,
                    },
                );
            }
            self.tail.col += width;
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
    pub width: usize,
    pub value: char,
}
