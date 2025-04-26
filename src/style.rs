use std::fmt::{Debug, Display};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub dim: bool,
    pub strikethrough: bool,
    pub fg_color: Option<Rgb>,
    pub bg_color: Option<Rgb>,
}

impl TerminalStyle {
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

    pub const fn new() -> Self {
        Self::RESET
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub const fn blink(mut self) -> Self {
        self.blink = true;
        self
    }

    pub const fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub const fn fg_color(mut self, color: Rgb) -> Self {
        self.fg_color = Some(color);
        self
    }

    pub const fn bg_color(mut self, color: Rgb) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn apply<T: Display>(self, text: T) -> String {
        format!("{}{}{}", self, text, Self::RESET)
    }

    pub fn apply_debug<T: Debug>(self, text: T) -> String {
        format!("{}{:?}{}", self, text, Self::RESET)
    }

    pub(crate) fn update<'a>(&mut self, s: &'a str) -> &'a str {
        let s = s
            .strip_prefix('[')
            .expect("Expected '[' after escape character '\\x1b' for valid ANSI escape sequence");
        let (s, remaining) = s
            .split_once('m')
            .expect("Expected 'm' terminator for ANSI escape sequence");
        match s {
            "0" => *self = TerminalStyle::default(),
            "1" => self.bold = true,
            "2" => self.dim = true,
            "3" => self.italic = true,
            "4" => self.underline = true,
            "5" => self.blink = true,
            "7" => self.reverse = true,
            "9" => self.strikethrough = true,
            _ => {
                let (fg, s) = if let Some(s) = s.strip_prefix("38;2;") {
                    (true, s)
                } else if let Some(s) = s.strip_prefix("48;2;") {
                    (false, s)
                } else {
                    panic!(
                        "Unsupported ANSI color format - expected 38;2; (foreground) or 48;2; (background) TrueColor sequence"
                    );
                };

                let (r, s) = s.split_once(';').expect(
                    "Invalid RGB format in ANSI color - expected ';' separator after red component",
                );
                let (g, b) = s.split_once(';').expect("Invalid RGB format in ANSI color - expected ';' separator after green component");
                let r = r
                    .parse()
                    .expect("Invalid red color value in ANSI RGB sequence - expected u8 value");
                let g = g
                    .parse()
                    .expect("Invalid green color value in ANSI RGB sequence - expected u8 value");
                let b = b
                    .parse()
                    .expect("Invalid blue color value in ANSI RGB sequence - expected u8 value");
                if fg {
                    self.fg_color = Some(Rgb { r, g, b });
                } else {
                    self.bg_color = Some(Rgb { r, g, b });
                }
            }
        }

        remaining
    }
}

impl Display for TerminalStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == TerminalStyle::RESET {
            return write!(f, "\x1b[0m");
        }

        write!(f, "\x1b[")?;

        let mut first = true;
        let mut write_separator = |f: &mut std::fmt::Formatter<'_>| -> std::fmt::Result {
            if first {
                first = false;
                Ok(())
            } else {
                write!(f, ";")
            }
        };

        if self.bold {
            write_separator(f)?;
            write!(f, "1")?;
        }
        if self.dim {
            write_separator(f)?;
            write!(f, "2")?;
        }
        if self.italic {
            write_separator(f)?;
            write!(f, "3")?;
        }
        if self.underline {
            write_separator(f)?;
            write!(f, "4")?;
        }
        if self.blink {
            write_separator(f)?;
            write!(f, "5")?;
        }
        if self.reverse {
            write_separator(f)?;
            write!(f, "7")?;
        }
        if self.strikethrough {
            write_separator(f)?;
            write!(f, "9")?;
        }

        if let Some(color) = self.fg_color {
            write_separator(f)?;
            write!(f, "38;2;{};{};{}", color.r, color.g, color.b)?;
        }

        if let Some(color) = self.bg_color {
            write_separator(f)?;
            write!(f, "48;2;{};{};{}", color.r, color.g, color.b)?;
        }

        write!(f, "m")
    }
}

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
