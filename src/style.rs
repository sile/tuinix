use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

/// Styling options for terminal text output.
///
/// [`Style`] allows you to modify the appearance of text in terminal output
/// using ANSI escape sequences. It supports standard terminal formatting options
/// including bold, italic, underline, colors, and more.
///
/// # Examples
///
/// ```
/// // Create a basic frame
/// let size = tuinix::Size { rows: 10, cols: 40 };
/// let mut frame = tuinix::Frame::new(size);
///
/// // Create a simple green, bold text style
/// let style = tuinix::Style::new()
///     .bold()
///     .fg_color(tuinix::Color::GREEN);
///
/// // Write styled text to the frame
/// let mut at = tuinix::Position::ORIGIN;
/// for c in "This text is bold and green".chars() {
///     at = frame.put_char(at, tuinix::Char::new(c, 1, style).expect("valid char"));
/// }
///
/// // Create another style for highlighting
/// let highlight = tuinix::Style::new()
///     .bg_color(tuinix::Color::YELLOW)
///     .fg_color(tuinix::Color::BLACK);
///
/// for c in "Important information".chars() {
///     at = frame.put_char(at, tuinix::Char::new(c, 1, highlight).expect("valid char"));
/// }
/// ```
///
/// # Style Application
///
/// A [`Frame`](crate::Frame) stores a complete [`Style`] for each character, and
/// rendering switches styles by emitting the full style rather than a patch. A
/// style therefore overrides whatever came before it completely: applying
/// `underline()` after `bold()` does not produce bold and underlined text, only
/// underlined text.
///
/// ```
/// let size = tuinix::Size { rows: 24, cols: 80 };
/// let mut frame = tuinix::Frame::new(size);
///
/// // This will produce text that is ONLY underlined, not bold+underlined
/// let bold = tuinix::Style::new().bold();
/// let underline = tuinix::Style::new().underline();
///
/// let mut at = tuinix::Position::ORIGIN;
/// for c in "This is bold.".chars() {
///     at = frame.put_char(at, tuinix::Char::new(c, 1, bold).expect("valid char"));
/// }
/// for c in "This is only underlined (not bold).".chars() {
///     at = frame.put_char(at, tuinix::Char::new(c, 1, underline).expect("valid char"));
/// }
///
/// // To apply multiple styles, combine them in a single Style instance
/// let bold_and_underlined = tuinix::Style::new().bold().underline();
/// for c in "This is both bold and underlined.".chars() {
///     at = frame.put_char(at, tuinix::Char::new(c, 1, bold_and_underlined).expect("valid char"));
/// }
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    /// Whether the text should be displayed in bold.
    pub bold: bool,

    /// Whether the text should be displayed in italic.
    pub italic: bool,

    /// Whether the text should be underlined.
    pub underline: bool,

    /// Whether the text should blink.
    pub blink: bool,

    /// Whether the foreground and background colors should be swapped.
    pub reverse: bool,

    /// Whether the text should be displayed with reduced intensity.
    pub dim: bool,

    /// Whether the text should have a line through it.
    pub strikethrough: bool,

    /// The foreground (text) color, if specified.
    pub fg_color: Option<Color>,

    /// The background color, if specified.
    pub bg_color: Option<Color>,
}

impl Style {
    /// A style with every formatting option disabled.
    ///
    /// It is equal to [`Style::new()`] and can be used to reset all terminal
    /// styling.
    pub const RESET: Self = Self {
        bold: false,
        italic: false,
        underline: false,
        blink: false,
        reverse: false,
        dim: false,
        strikethrough: false,
        fg_color: None,
        bg_color: None,
    };

    /// Makes a new style with all formatting options disabled.
    ///
    /// The result is equal to [`Style::RESET`] and can be used as a starting
    /// point to build more complex styles through the builder methods.
    ///
    /// # Examples
    ///
    /// ```
    /// let size = tuinix::Size { rows: 1, cols: 5 };
    /// let mut frame = tuinix::Frame::new(size);
    ///
    /// let style = tuinix::Style::new()
    ///     .bold()
    ///     .fg_color(tuinix::Color::GREEN);
    ///
    /// let mut at = tuinix::Position::ORIGIN;
    /// for c in "hello".chars() {
    ///     at = frame.put_char(at, tuinix::Char::new(c, 1, style).expect("valid char"));
    /// }
    /// ```
    pub const fn new() -> Self {
        Self::RESET
    }

    /// Sets the text to be displayed in bold style.
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Sets the text to be displayed in italic style.
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Sets the text to be underlined.
    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Sets the text to blink.
    pub const fn blink(mut self) -> Self {
        self.blink = true;
        self
    }

    /// Swaps foreground and background colors of the text.
    pub const fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// Sets the text to be displayed with reduced intensity.
    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    /// Sets the text to have a line through it.
    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Sets the foreground (text) color.
    pub const fn fg_color(mut self, color: Color) -> Self {
        self.fg_color = Some(color);
        self
    }

    /// Sets the background color behind the text.
    pub const fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }
}

impl Display for Style {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\x1b[0")?;

        if self.bold {
            write!(f, ";1")?;
        }
        if self.dim {
            write!(f, ";2")?;
        }
        if self.italic {
            write!(f, ";3")?;
        }
        if self.underline {
            write!(f, ";4")?;
        }
        if self.blink {
            write!(f, ";5")?;
        }
        if self.reverse {
            write!(f, ";7")?;
        }
        if self.strikethrough {
            write!(f, ";9")?;
        }
        if let Some(color) = self.fg_color {
            write!(f, ";38;{color}")?;
        }
        if let Some(color) = self.bg_color {
            write!(f, ";48;{color}")?;
        }

        write!(f, "m")
    }
}

impl FromStr for Style {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut this = Self::default();
        let error = || format!("invalid or unsupported ANSI escape sequence: {s:?}");
        let is_delimiter = |s: &&str| s.starts_with([';', 'm']);

        let mut s = s.strip_prefix("\x1b[0").ok_or_else(error)?;
        if let Some(s0) = s.strip_prefix(";1").filter(is_delimiter) {
            this.bold = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";2").filter(is_delimiter) {
            this.dim = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";3").filter(is_delimiter) {
            this.italic = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";4").filter(is_delimiter) {
            this.underline = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";5").filter(is_delimiter) {
            this.blink = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";7").filter(is_delimiter) {
            this.reverse = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";9").filter(is_delimiter) {
            this.strikethrough = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";38;5;") {
            let (index, s0) = s0
                .match_indices(&[';', 'm'])
                .next()
                .map(|(i, _)| s0.split_at(i))
                .ok_or_else(error)?;
            let index = index.parse().map_err(|_| error())?;
            this.fg_color = Some(Color::Indexed(index));
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";38;2;") {
            let (r, s0) = s0.split_once(';').ok_or_else(error)?;
            let (g, s0) = s0.split_once(';').ok_or_else(error)?;
            let (b, s0) = s0
                .match_indices(&[';', 'm'])
                .next()
                .map(|(i, _)| s0.split_at(i))
                .ok_or_else(error)?;
            let r = r.parse().map_err(|_| error())?;
            let g = g.parse().map_err(|_| error())?;
            let b = b.parse().map_err(|_| error())?;
            this.fg_color = Some(Color::Rgb(r, g, b));
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";48;5;") {
            let (index, s0) = s0
                .match_indices(&[';', 'm'])
                .next()
                .map(|(i, _)| s0.split_at(i))
                .ok_or_else(error)?;
            let index = index.parse().map_err(|_| error())?;
            this.bg_color = Some(Color::Indexed(index));
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";48;2;") {
            let (r, s0) = s0.split_once(';').ok_or_else(error)?;
            let (g, s0) = s0.split_once(';').ok_or_else(error)?;
            let (b, s0) = s0
                .match_indices(&[';', 'm'])
                .next()
                .map(|(i, _)| s0.split_at(i))
                .ok_or_else(error)?;
            let r = r.parse().map_err(|_| error())?;
            let g = g.parse().map_err(|_| error())?;
            let b = b.parse().map_err(|_| error())?;
            this.bg_color = Some(Color::Rgb(r, g, b));
            s = s0;
        }

        if s != "m" {
            return Err(error());
        }
        Ok(this)
    }
}

/// A color a frame can be drawn in.
///
/// A color is either an entry in the terminal's palette or a 24-bit RGB value.
/// The palette is the terminal's, not tuinix's: an [`Color::Indexed`] is drawn
/// as whatever the terminal says that entry is, which is why the named
/// constants are indices rather than RGB triples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// An entry in the terminal's palette (`0..=255`).
    Indexed(u8),

    /// A 24-bit color.
    Rgb(u8, u8, u8),
}

impl Color {
    /// ANSI black (color index 0).
    pub const BLACK: Self = Self::Indexed(0);

    /// ANSI red (color index 1).
    pub const RED: Self = Self::Indexed(1);

    /// ANSI green (color index 2).
    pub const GREEN: Self = Self::Indexed(2);

    /// ANSI yellow (color index 3).
    pub const YELLOW: Self = Self::Indexed(3);

    /// ANSI blue (color index 4).
    pub const BLUE: Self = Self::Indexed(4);

    /// ANSI magenta (color index 5).
    pub const MAGENTA: Self = Self::Indexed(5);

    /// ANSI cyan (color index 6).
    pub const CYAN: Self = Self::Indexed(6);

    /// ANSI white (color index 7).
    pub const WHITE: Self = Self::Indexed(7);

    /// ANSI bright black (color index 8).
    pub const BRIGHT_BLACK: Self = Self::Indexed(8);

    /// ANSI bright red (color index 9).
    pub const BRIGHT_RED: Self = Self::Indexed(9);

    /// ANSI bright green (color index 10).
    pub const BRIGHT_GREEN: Self = Self::Indexed(10);

    /// ANSI bright yellow (color index 11).
    pub const BRIGHT_YELLOW: Self = Self::Indexed(11);

    /// ANSI bright blue (color index 12).
    pub const BRIGHT_BLUE: Self = Self::Indexed(12);

    /// ANSI bright magenta (color index 13).
    pub const BRIGHT_MAGENTA: Self = Self::Indexed(13);

    /// ANSI bright cyan (color index 14).
    pub const BRIGHT_CYAN: Self = Self::Indexed(14);

    /// ANSI bright white (color index 15).
    pub const BRIGHT_WHITE: Self = Self::Indexed(15);
}

impl Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Indexed(index) => write!(f, "5;{index}"),
            Self::Rgb(r, g, b) => write!(f, "2;{r};{g};{b}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_style() {
        let style: Style = "\x1b[0;1;38;2;0;255;0m".parse().expect("invalid");
        assert!(style.bold);
        assert_eq!(style.fg_color, Some(Color::Rgb(0, 255, 0)));

        let style: Style = "\x1b[0;38;2;0;0;0;48;2;255;255;0m"
            .parse()
            .expect("invalid");
        assert_eq!(style.fg_color, Some(Color::Rgb(0, 0, 0)));
        assert_eq!(style.bg_color, Some(Color::Rgb(255, 255, 0)));

        let style: Style = "\x1b[0;38;5;2;48;5;255m".parse().expect("invalid");
        assert_eq!(style.fg_color, Some(Color::Indexed(2)));
        assert_eq!(style.bg_color, Some(Color::Indexed(255)));
    }

    #[test]
    fn display_style() {
        assert_eq!(
            Style::new().fg_color(Color::GREEN).to_string(),
            "\x1b[0;38;5;2m"
        );
        assert_eq!(
            Style::new().fg_color(Color::Rgb(0, 255, 0)).to_string(),
            "\x1b[0;38;2;0;255;0m"
        );
        assert_eq!(
            Style::new().bg_color(Color::Indexed(255)).to_string(),
            "\x1b[0;48;5;255m"
        );
    }
}
