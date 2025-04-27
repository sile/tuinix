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
/// use tuinix::{Rgb, TerminalStyle};
///
/// // Create styled text
/// let styled = TerminalStyle::new()
///     .bold()
///     .fg_color(Rgb::RED)
///     .apply("This is bold red text");
///
/// // Apply multiple styles
/// let complex = TerminalStyle::new()
///     .underline()
///     .italic()
///     .bg_color(Rgb::BLUE)
///     .apply("Underlined italic text with blue background");
/// ```
///
/// The styling is applied by wrapping the text with the appropriate ANSI escape
/// sequences. All styles are automatically reset at the end of the text.
///
/// You can also format styled text directly using the `Display` trait:
///
/// ```
/// use tuinix::{Rgb, TerminalStyle};
///
/// // Format styled text directly with format!
/// let style = TerminalStyle::new().bold().fg_color(Rgb::GREEN);
/// let formatted = format!("{}{}{}", style, "Direct formatting", TerminalStyle::RESET);
///
/// // This approach gives more flexibility for complex formatting
/// let warning_style = TerminalStyle::new().bold().fg_color(Rgb::YELLOW);
/// let error_style = TerminalStyle::new().bold().fg_color(Rgb::RED);
///
/// let message = format!(
///     "{}WARNING:{} This operation {}might be dangerous{}!",
///     warning_style,
///     TerminalStyle::RESET,
///     error_style,
///     TerminalStyle::RESET
/// );
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
    pub fg_color: Option<Rgb>,

    /// The background color, if specified.
    pub bg_color: Option<Rgb>,
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
    /// use tuinix::{Rgb, TerminalStyle};
    ///
    /// let style = TerminalStyle::new()
    ///     .bold()
    ///     .fg_color(Rgb::GREEN);
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
    pub const fn fg_color(mut self, color: Rgb) -> Self {
        self.fg_color = Some(color);
        self
    }

    /// Sets the background color behind the text.
    pub const fn bg_color(mut self, color: Rgb) -> Self {
        self.bg_color = Some(color);
        self
    }

    /// Applies this style to the provided text.
    ///
    /// This method wraps the provided text with the current style and a reset sequence,
    /// returning a formatted string that will display with the specified styling in
    /// compatible terminals.
    ///
    /// # Examples
    ///
    /// ```
    /// use tuinix::{Rgb, TerminalStyle};
    ///
    /// // Apply style to text
    /// let styled_text = TerminalStyle::new()
    ///     .bold()
    ///     .fg_color(Rgb::BLUE)
    ///     .apply("Important message");
    ///
    /// // The result contains ANSI escape sequences that will display
    /// // "Important message" in bold blue text in the terminal
    /// ```
    ///
    /// This is equivalent to using the format macro with explicit style and reset:
    /// ```
    /// # use tuinix::{Rgb, TerminalStyle};
    /// let style = TerminalStyle::new().bold().fg_color(Rgb::BLUE);
    /// let styled_text = format!("{}{}{}", style, "Important message", TerminalStyle::RESET);
    /// ```
    // TODO: delete?
    pub fn apply<T: Display>(self, text: T) -> String {
        format!("{}{}{}", self, text, Self::RESET)
    }

    /// Similar to [`TerminalStyle::apply()`], but formats the given text using the [`Debug`] trait representation.
    pub fn apply_debug<T: Debug>(self, text: T) -> String {
        format!("{}{:?}{}", self, text, Self::RESET)
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
        let error = || "invalid or unsupported ANSI escape sequence".to_string();

        let mut s = s.strip_prefix("\x1b[0").ok_or_else(error)?;
        if let Some(s0) = s.strip_prefix(";1") {
            this.bold = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";2") {
            this.dim = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";3") {
            this.italic = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";4") {
            this.underline = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";5") {
            this.blink = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";7") {
            this.reverse = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";9") {
            this.strikethrough = true;
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";38;2;") {
            let (r, s0) = s0.split_once(';').ok_or_else(error)?;
            let (g, s0) = s0.split_once(';').ok_or_else(error)?;
            let (b, s0) = s0.split_once(';').ok_or_else(error)?;
            let r = r.parse().map_err(|_| error())?;
            let g = g.parse().map_err(|_| error())?;
            let b = b.parse().map_err(|_| error())?;
            this.fg_color = Some(Rgb::new(r, g, b));
            s = s0;
        }
        if let Some(s0) = s.strip_prefix(";48;2;") {
            let (r, s0) = s0.split_once(';').ok_or_else(error)?;
            let (g, s0) = s0.split_once(';').ok_or_else(error)?;
            let (b, s0) = s0.split_once(';').ok_or_else(error)?;
            let r = r.parse().map_err(|_| error())?;
            let g = g.parse().map_err(|_| error())?;
            let b = b.parse().map_err(|_| error())?;
            this.bg_color = Some(Rgb::new(r, g, b));
            s = s0;
        }

        if s != "m" {
            return Err(error());
        }
        Ok(this)
    }
}

// TODO: TerminalColor?
/// RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    /// Red component.
    pub r: u8,

    /// Green component.
    pub g: u8,

    /// Blue component.
    pub b: u8,
}

impl Rgb {
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

    /// Makes a new [`Rgb`] instance.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}
