use std::collections::BTreeMap;

use unicode_width::UnicodeWidthChar;

use crate::terminal::TerminalSize;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    size: TerminalSize,
    cursor: TerminalPosition,
    chars: BTreeMap<TerminalPosition, TerminalChar>,
    current_style: TerminalStyle,
}

impl TerminalFrame {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            cursor: TerminalPosition::default(),
            chars: BTreeMap::new(),
            current_style: TerminalStyle::default(),
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    fn push_char(&mut self, c: char) {
        let Some(width) = c.width() else {
            // control char
            return;
        };

        let c = TerminalChar {
            value: c,
            style: self.current_style,
        };
        self.chars.insert(self.cursor, c);
        self.cursor.col += width;
    }
}

impl std::fmt::Write for TerminalFrame {
    fn write_str(&mut self, mut s: &str) -> std::fmt::Result {
        loop {
            for (i, c) in s.char_indices() {
                match c {
                    '\n' => {
                        self.cursor.row += 1;
                        self.cursor.col = 0;
                    }
                    '\x1b' => {
                        s = self.current_style.update(&s[i + 1..]);
                        continue;
                    }
                    _ => {
                        self.push_char(c);
                    }
                }
            }
            break;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalPosition {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalChar {
    pub value: char,
    pub style: TerminalStyle,
}

// TODO: attrs?
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
    fn update<'a>(&mut self, s: &'a str) -> &'a str {
        let s = s
            .strip_prefix('[')
            .expect("Expected '[' after escape character '\\x1b' for valid ANSI escape sequence");
        let (code, s) = s
            .split_once('m')
            .expect("Expected 'm' terminator for ANSI escape sequence");
        if code == "0" {
            *self = TerminalStyle::default();
            return s;
        }

        for part in code.split(';') {
            match part {
                "1" => self.bold = true,
                "2" => self.dim = true,
                "3" => self.italic = true,
                "4" => self.underline = true,
                "5" => self.blink = true,
                "7" => self.reverse = true,
                "9" => self.strikethrough = true,
                "22" => {
                    self.bold = false;
                    self.dim = false;
                }
                "23" => self.italic = false,
                "24" => self.underline = false,
                "25" => self.blink = false,
                "27" => self.reverse = false,
                "29" => self.strikethrough = false,
                "39" => self.fg_color = None,
                "49" => self.bg_color = None,
                // 8-bit color for foreground
                "38" if code.starts_with("38;5;") => {
                    if let Some(color_code) = code
                        .strip_prefix("38;5;")
                        .and_then(|s| s.parse::<u8>().ok())
                    {
                        // Convert 8-bit color to RGB (simplified)
                        self.fg_color = Some(Rgb {
                            r: color_code,
                            g: color_code,
                            b: color_code,
                        });
                    }
                }
                // 8-bit color for background
                "48" if code.starts_with("48;5;") => {
                    if let Some(color_code) = code
                        .strip_prefix("48;5;")
                        .and_then(|s| s.parse::<u8>().ok())
                    {
                        // Convert 8-bit color to RGB (simplified)
                        self.bg_color = Some(Rgb {
                            r: color_code,
                            g: color_code,
                            b: color_code,
                        });
                    }
                }
                // 24-bit RGB color for foreground
                "38" if code.contains("38;2;") => {
                    let rgb_parts: Vec<&str> = code.split(';').collect();
                    if rgb_parts.len() >= 5 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            rgb_parts[2].parse::<u8>(),
                            rgb_parts[3].parse::<u8>(),
                            rgb_parts[4].parse::<u8>(),
                        ) {
                            self.fg_color = Some(Rgb { r, g, b });
                        }
                    }
                }
                // 24-bit RGB color for background
                "48" if code.contains("48;2;") => {
                    let rgb_parts: Vec<&str> = code.split(';').collect();
                    if rgb_parts.len() >= 5 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            rgb_parts[2].parse::<u8>(),
                            rgb_parts[3].parse::<u8>(),
                            rgb_parts[4].parse::<u8>(),
                        ) {
                            self.bg_color = Some(Rgb { r, g, b });
                        }
                    }
                }
                // Basic 16 colors for foreground
                x if x.parse::<u8>().map_or(false, |n| (30..=37).contains(&n)) => {
                    if let Ok(n) = x.parse::<u8>() {
                        // Convert basic ANSI color to RGB (simplified mapping)
                        let color_value = (n - 30) * 32;
                        self.fg_color = Some(Rgb {
                            r: color_value,
                            g: color_value,
                            b: color_value,
                        });
                    }
                }
                // Basic 16 colors for background
                x if x.parse::<u8>().map_or(false, |n| (40..=47).contains(&n)) => {
                    if let Ok(n) = x.parse::<u8>() {
                        // Convert basic ANSI color to RGB (simplified mapping)
                        let color_value = (n - 40) * 32;
                        self.bg_color = Some(Rgb {
                            r: color_value,
                            g: color_value,
                            b: color_value,
                        });
                    }
                }
                // Bright colors for foreground
                x if x.parse::<u8>().map_or(false, |n| (90..=97).contains(&n)) => {
                    if let Ok(n) = x.parse::<u8>() {
                        // Convert bright ANSI color to RGB (simplified mapping)
                        let color_value = (n - 90) * 32 + 128;
                        self.fg_color = Some(Rgb {
                            r: color_value,
                            g: color_value,
                            b: color_value,
                        });
                    }
                }
                // Bright colors for background
                x if x.parse::<u8>().map_or(false, |n| (100..=107).contains(&n)) => {
                    if let Ok(n) = x.parse::<u8>() {
                        // Convert bright ANSI color to RGB (simplified mapping)
                        let color_value = (n - 100) * 32 + 128;
                        self.bg_color = Some(Rgb {
                            r: color_value,
                            g: color_value,
                            b: color_value,
                        });
                    }
                }
                _ => {} // Ignore unsupported codes
            }
        }

        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
