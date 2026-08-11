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
    use std::{cell::Cell, collections::BTreeMap, fmt::Write};

    use unicode_width::UnicodeWidthChar;

    use super::*;

    struct UnicodeCharWidthEstimator;

    impl EstimateCharWidth for UnicodeCharWidthEstimator {
        fn estimate_char_width(&self, c: char) -> usize {
            c.width().unwrap_or_default()
        }
    }

    const WIDE_CHARS: &[char] = &[
        'あ', 'い', 'う', 'え', 'お', '界', '日', '本', '漢', '字', '語',
    ];

    const ZERO_WIDTH_CHARS: &[char] = &['\u{301}', '\u{200d}', '\u{20dd}'];

    fn sample_pbt_style(ctx: &mut noprop::TestCaseContext) -> TerminalStyle {
        const SETTERS: [fn(TerminalStyle) -> TerminalStyle; 7] = [
            TerminalStyle::bold,
            TerminalStyle::italic,
            TerminalStyle::underline,
            TerminalStyle::blink,
            TerminalStyle::reverse,
            TerminalStyle::dim,
            TerminalStyle::strikethrough,
        ];
        let mut style = TerminalStyle::new();
        for setter in SETTERS {
            if noprop::sample_bool(ctx) {
                style = setter(style);
            }
        }
        if noprop::sample_bool(ctx) {
            style = style.fg_color(crate::TerminalColor::new(
                noprop::sample_u8(ctx),
                noprop::sample_u8(ctx),
                noprop::sample_u8(ctx),
            ));
        }
        if noprop::sample_bool(ctx) {
            style = style.bg_color(crate::TerminalColor::new(
                noprop::sample_u8(ctx),
                noprop::sample_u8(ctx),
                noprop::sample_u8(ctx),
            ));
        }
        style
    }

    /// Draws a random string that mixes visible ASCII, newlines, style
    /// escape sequences, wide characters, and zero-width characters.
    ///
    /// Visible characters exclude space so that no stored character
    /// can be confused with `TerminalChar::BLANK`.
    fn sample_pbt_text(ctx: &mut noprop::TestCaseContext) -> String {
        let mut text = String::new();
        let n_chars =
            noprop::sample_with_boundaries(ctx, &[0usize, 48], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=48)
            });
        for _ in 0..n_chars {
            match noprop::sample_weighted_index(ctx, &[4, 1, 1, 1, 1]) {
                0 => {
                    text.push(
                        char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                            .expect("valid ASCII"),
                    );
                }
                1 => text.push('\n'),
                2 => text.push_str(&sample_pbt_style(ctx).to_string()),
                3 => text.push(noprop::sample_choice(ctx, WIDE_CHARS)),
                _ => text.push(noprop::sample_choice(ctx, ZERO_WIDTH_CHARS)),
            }
        }
        text
    }

    /// A model of `TerminalFrame::write_str`: tracks the cursor, the
    /// stored characters, the current style, and which interesting
    /// behaviors were observed.
    struct FrameModel {
        tail: TerminalPosition,
        data: BTreeMap<TerminalPosition, TerminalChar>,
        current_style: TerminalStyle,
        escape_sequence: String,
        clipped: bool,
        newline: bool,
        zero_width: bool,
    }

    impl FrameModel {
        fn new() -> Self {
            Self {
                tail: TerminalPosition::ZERO,
                data: BTreeMap::new(),
                current_style: TerminalStyle::new(),
                escape_sequence: String::new(),
                clipped: false,
                newline: false,
                zero_width: false,
            }
        }

        fn write(&mut self, s: &str, size: TerminalSize, estimator: &impl EstimateCharWidth) {
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
                }
                match c {
                    '\x1b' => self.escape_sequence.push(c),
                    '\n' => {
                        self.tail.row += 1;
                        self.tail.col = 0;
                        self.newline = true;
                    }
                    c => {
                        let Some(width) = NonZeroUsize::new(estimator.estimate_char_width(c))
                        else {
                            self.zero_width = true;
                            continue;
                        };
                        if self.tail.row < size.rows && self.tail.col + width.get() <= size.cols {
                            self.data.insert(
                                self.tail,
                                TerminalChar {
                                    style: self.current_style,
                                    width,
                                    value: c,
                                },
                            );
                        } else {
                            self.clipped = true;
                        }
                        self.tail.col += width.get();
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

    /// The cursor and the stored characters of a frame after
    /// `write_str` must match the model, including wide characters,
    /// zero-width characters, style changes, newlines, and clipping at
    /// the frame boundary.
    #[test]
    fn pbt_write_content_matches_model() -> noprop::TestResult {
        let observed_wide = Cell::new(false);
        let observed_zero_width = Cell::new(false);
        let observed_styled = Cell::new(false);
        let observed_clipped = Cell::new(false);
        let observed_newline = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let size = sample_pbt_size(ctx);
            let text = sample_pbt_text(ctx);
            let mut model = FrameModel::new();
            model.write(&text, size, &UnicodeCharWidthEstimator);
            let mut frame =
                TerminalFrame::with_char_width_estimator(size, UnicodeCharWidthEstimator);
            frame.write_str(&text).unwrap();
            assert_eq!(frame.cursor(), model.tail, "cursor mismatch for {text:?}");
            let actual: BTreeMap<_, _> = frame
                .chars()
                .filter(|(_, c)| *c != TerminalChar::BLANK)
                .collect();
            assert_eq!(actual, model.data, "content mismatch for {text:?}");
            if model.data.values().any(|c| c.width.get() > 1) {
                observed_wide.set(true);
            }
            if model.data.values().any(|c| c.style != TerminalStyle::new()) {
                observed_styled.set(true);
            }
            if model.clipped {
                observed_clipped.set(true);
            }
            if model.newline {
                observed_newline.set(true);
            }
            if model.zero_width {
                observed_zero_width.set(true);
            }
            Ok(())
        })?;
        assert!(
            observed_wide.get(),
            "no case wrote a wide character\n{runner}"
        );
        assert!(
            observed_zero_width.get(),
            "no case wrote a zero-width character\n{runner}"
        );
        assert!(
            observed_styled.get(),
            "no case wrote a styled character\n{runner}"
        );
        assert!(
            observed_clipped.get(),
            "no case clipped a character\n{runner}"
        );
        assert!(observed_newline.get(), "no case wrote a newline\n{runner}");
        Ok(())
    }

    /// `TerminalFrame::draw` must match a model that replays the
    /// overlap handling: a partially overlapped character is removed,
    /// the cells covered by the drawn character are cleared, and
    /// characters drawn outside the frame are ignored.
    #[test]
    fn pbt_draw_matches_model() -> noprop::TestResult {
        let observed_overlap = Cell::new(false);
        let observed_clipped = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            // Half of the cases force a partial overlap structurally: a
            // wide character whose second cell is overwritten by a drawn
            // character. Relying on random generation alone made the
            // overlap gate flaky, since a partial overlap requires a
            // width-2 character to align with the first column of a
            // drawn character.
            let structured = noprop::sample_bool(ctx);
            let (size, dest_text, src_text, position) = if structured {
                (
                    TerminalSize::rows_cols(1, 4),
                    "あ".to_string(),
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
            let mut dest =
                TerminalFrame::with_char_width_estimator(size, UnicodeCharWidthEstimator);
            dest.write_str(&dest_text).unwrap();
            let mut model = FrameModel::new();
            model.write(&dest_text, size, &UnicodeCharWidthEstimator);
            let mut expected = model.data;
            let mut src = TerminalFrame::with_char_width_estimator(size, UnicodeCharWidthEstimator);
            src.write_str(&src_text).unwrap();
            let mut removals = 0usize;
            let mut skipped = 0usize;
            for (src_pos, c) in src.chars() {
                let target_pos = position + src_pos;
                if !size.contains(target_pos) {
                    skipped += 1;
                    continue;
                }
                if let Some((&prev_pos, prev_c)) = expected.range(..target_pos).next_back() {
                    let end_pos = prev_pos + TerminalPosition::col(prev_c.width.get());
                    if target_pos < end_pos {
                        expected.remove(&prev_pos);
                        removals += 1;
                    }
                }
                for i in 0..c.width.get() {
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
