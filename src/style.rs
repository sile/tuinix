use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

/// Styling options for terminal text output.
///
/// [`TerminalStyle`] allows you to modify the appearance of text in terminal output
/// using ANSI escape sequences. It supports standard terminal formatting options
/// including bold, italic, underline, colors, and more.
///
/// # Examples
///
/// ```
/// use std::fmt::Write;
/// use tuinix::{TerminalColor, TerminalFrame, TerminalSize, TerminalStyle};
///
/// // Create a basic terminal frame
/// let size = TerminalSize { rows: 10, cols: 40 };
/// let mut frame = TerminalFrame::new(size);
///
/// // Create a simple green, bold text style
/// let style = TerminalStyle::new()
///     .bold()
///     .fg_color(TerminalColor::GREEN);
///
/// // Write styled text to the frame
/// writeln!(frame, "{}This text is bold and green{}", style, TerminalStyle::RESET)?;
///
/// // Create another style for highlighting
/// let highlight = TerminalStyle::new()
///     .bg_color(TerminalColor::YELLOW)
///     .fg_color(TerminalColor::BLACK);
///
/// writeln!(frame, "{}Important information{}", highlight, TerminalStyle::RESET)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
///
/// # Style Application
///
/// When applying styles, each new style overrides any previous style completely.
/// This means that applying a style like `underline()` after `bold()` won't result
/// in text that is both bold and underlined - only the underline will be applied.
///
/// ```
/// use std::fmt::Write;
/// use tuinix::{TerminalFrame, TerminalSize, TerminalStyle};
///
/// let size = TerminalSize { rows: 24, cols: 80 };
/// let mut frame = TerminalFrame::new(size);
///
/// // This will produce text that is ONLY underlined, not bold+underlined
/// let bold = TerminalStyle::new().bold();
/// let underline = TerminalStyle::new().underline();
///
/// writeln!(frame, "{}This is bold.", bold)?;
/// writeln!(frame, "{}This is only underlined (not bold).", underline)?;
///
/// // To apply multiple styles, combine them in a single TerminalStyle instance
/// let bold_and_underlined = TerminalStyle::new().bold().underline();
/// writeln!(frame, " {}This is both bold and underlined.", bold_and_underlined)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalStyle {
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
    pub fg_color: Option<TerminalColor>,

    /// The background color, if specified.
    pub bg_color: Option<TerminalColor>,
}

impl TerminalStyle {
    /// An alias of [`TerminalStyle::new()`] that
    /// can be used to reset all terminal styling.
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

    /// Makes a new terminal style with all formatting options disabled.
    ///
    /// This returns a style instance equivalent to [`TerminalStyle::RESET`],
    /// which can be used as a starting point to build more complex styles
    /// through the builder methods.
    ///
    /// # Examples
    ///
    /// ```
    /// use tuinix::{TerminalColor, TerminalStyle};
    ///
    /// let style = TerminalStyle::new()
    ///     .bold()
    ///     .fg_color(TerminalColor::GREEN);
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
    pub const fn fg_color(mut self, color: TerminalColor) -> Self {
        self.fg_color = Some(color);
        self
    }

    /// Sets the background color behind the text.
    pub const fn bg_color(mut self, color: TerminalColor) -> Self {
        self.bg_color = Some(color);
        self
    }
}

impl Display for TerminalStyle {
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
            write!(f, ";38;2;{};{};{}", color.r, color.g, color.b)?;
        }
        if let Some(color) = self.bg_color {
            write!(f, ";48;2;{};{};{}", color.r, color.g, color.b)?;
        }

        write!(f, "m")
    }
}

impl FromStr for TerminalStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut this = Self::default();
        let error = || format!("invalid or unsupported ANSI escape sequence: {:?}", s);
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
            this.fg_color = Some(TerminalColor::new(r, g, b));
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
            this.bg_color = Some(TerminalColor::new(r, g, b));
            s = s0;
        }

        if s != "m" {
            return Err(error());
        }
        Ok(this)
    }
}

/// Terminal color (RGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalColor {
    /// Red component.
    pub r: u8,

    /// Green component.
    pub g: u8,

    /// Blue component.
    pub b: u8,
}

impl TerminalColor {
    /// ANSI black color (RGB: 0, 0, 0).
    pub const BLACK: Self = Self::new(0, 0, 0);

    /// ANSI red color (RGB: 255, 0, 0).
    pub const RED: Self = Self::new(255, 0, 0);

    /// ANSI green color (RGB: 0, 255, 0).
    pub const GREEN: Self = Self::new(0, 255, 0);

    /// ANSI yellow color (RGB: 255, 255, 0).
    pub const YELLOW: Self = Self::new(255, 255, 0);

    /// ANSI blue color (RGB: 0, 0, 255).
    pub const BLUE: Self = Self::new(0, 0, 255);

    /// ANSI magenta color (RGB: 255, 0, 255).
    pub const MAGENTA: Self = Self::new(255, 0, 255);

    /// ANSI cyan color (RGB: 0, 255, 255).
    pub const CYAN: Self = Self::new(0, 255, 255);

    /// ANSI white color (RGB: 255, 255, 255).
    pub const WHITE: Self = Self::new(255, 255, 255);

    /// ANSI bright black color (gray) (RGB: 128, 128, 128).
    pub const BRIGHT_BLACK: Self = Self::new(128, 128, 128);

    /// ANSI bright red color (RGB: 255, 100, 100).
    pub const BRIGHT_RED: Self = Self::new(255, 100, 100);

    /// ANSI bright green color (RGB: 100, 255, 100).
    pub const BRIGHT_GREEN: Self = Self::new(100, 255, 100);

    /// ANSI bright yellow color (RGB: 255, 255, 100).
    pub const BRIGHT_YELLOW: Self = Self::new(255, 255, 100);

    /// ANSI bright blue color (RGB: 100, 100, 255).
    pub const BRIGHT_BLUE: Self = Self::new(100, 100, 255);

    /// ANSI bright magenta color (RGB: 255, 100, 255).
    pub const BRIGHT_MAGENTA: Self = Self::new(255, 100, 255);

    /// ANSI bright cyan color (RGB: 100, 255, 255).
    pub const BRIGHT_CYAN: Self = Self::new(100, 255, 255);

    /// ANSI bright white color (RGB: 255, 255, 255).
    pub const BRIGHT_WHITE: Self = Self::new(255, 255, 255);

    /// Makes a new [`TerminalColor`] instance.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_style() {
        let style: TerminalStyle = "\x1b[0;1;38;2;0;255;0m".parse().expect("invalid");
        assert!(style.bold);
        assert_eq!(style.fg_color, Some(TerminalColor::GREEN));

        let style: TerminalStyle = "\x1b[0;38;2;0;0;0;48;2;255;255;0m"
            .parse()
            .expect("invalid");
        assert_eq!(style.fg_color, Some(TerminalColor::BLACK));
        assert_eq!(style.bg_color, Some(TerminalColor::YELLOW));
    }
}
