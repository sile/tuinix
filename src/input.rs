use std::io::Read;

/// User input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalInput {
    /// Keyboard input.
    Key(KeyInput),
}

/// Keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyInput {
    /// Indicates whether the Ctrl modifier key was pressed during the input.
    pub ctrl: bool,

    /// Indicates whether the Alt modifier key was pressed during the input.
    pub alt: bool,

    /// Key code representing which key was pressed.
    pub code: KeyCode,
}

/// Key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    /// Enter key.
    Enter,
    /// Escape key.
    Escape,
    /// Backspace key.
    Backspace,
    /// Tab key.
    Tab,
    /// BackTab key.
    BackTab,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page Up key.
    PageUp,
    /// Page Down key.
    PageDown,
    /// Character key.
    Char(char),
}

#[derive(Debug)]
pub struct InputReader<R> {
    inner: R,
    buf: Vec<u8>,
    buf_offset: usize,
}

impl<R: Read> InputReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: vec![0; 64],
            buf_offset: 0,
        }
    }

    pub fn inner(&self) -> &R {
        &self.inner
    }

    pub fn read_input(&mut self) -> std::io::Result<Option<TerminalInput>> {
        let read_size = self.inner.read(&mut self.buf[self.buf_offset..])?;
        if read_size == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }

        let size = self.buf_offset + read_size;
        let Some((input, consumed_size)) = parse_input(&self.buf[..size])? else {
            return Ok(None);
        };
        self.buf.copy_within(consumed_size..size, 0);
        self.buf_offset = 0;
        Ok(input)
    }
}

fn parse_input(bytes: &[u8]) -> std::io::Result<Option<(Option<TerminalInput>, usize)>> {
    if bytes.is_empty() {
        return Ok(None);
    }

    // Regular ASCII character
    if bytes[0] < 0x80 && bytes[0] != 0x1b && bytes[0] != 0x7f {
        // Control characters (Ctrl+A through Ctrl+Z)
        if bytes[0] < 0x20 {
            let ctrl = true;
            let code = match bytes[0] {
                0x0D => KeyCode::Enter,     // Enter
                0x09 => KeyCode::Tab,       // Tab
                0x08 => KeyCode::Backspace, // Backspace (Ctrl+H)
                c => KeyCode::Char((c + 0x60) as char),
            };
            return Ok(Some((
                Some(TerminalInput::Key(KeyInput {
                    ctrl,
                    alt: false,
                    code,
                })),
                1,
            )));
        }

        // Regular ASCII characters
        return Ok(Some((
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char(bytes[0] as char),
            })),
            1,
        )));
    }

    // Special keys and escape sequences
    match bytes[0] {
        // Escape key or start of escape sequence
        0x1b => {
            // For a standalone ESC press, we need to wait and see if more bytes follow
            if bytes.len() == 1 {
                return Ok(None); // Need more bytes
            }

            // Alt + character (ESC followed by a regular character)
            if bytes[1] < 0x80 && bytes[1] != 0x1b && bytes[1] != 0x5b && bytes[1] != 0x4f {
                let c = bytes[1] as char;
                let code = if bytes[1] < 0x20 {
                    // Control characters with Alt
                    match bytes[1] {
                        0x0D => KeyCode::Enter,
                        0x09 => KeyCode::Tab,
                        0x08 => KeyCode::Backspace,
                        c => KeyCode::Char((c + 0x60) as char),
                    }
                } else {
                    KeyCode::Char(c)
                };

                return Ok(Some((
                    Some(TerminalInput::Key(KeyInput {
                        ctrl: bytes[1] < 0x20,
                        alt: true,
                        code,
                    })),
                    2,
                )));
            }

            // ESC [ sequences (most function keys, arrow keys, etc.)
            if bytes[1] == b'[' {
                // Need at least 3 bytes for the basic arrow keys (ESC [ A)
                if bytes.len() < 3 {
                    return Ok(None); // Need more bytes
                }

                // Arrow keys: ESC [ A, ESC [ B, ESC [ C, ESC [ D
                match bytes[2] {
                    b'A' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Up,
                            })),
                            3,
                        )));
                    }
                    b'B' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Down,
                            })),
                            3,
                        )));
                    }
                    b'C' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Right,
                            })),
                            3,
                        )));
                    }
                    b'D' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Left,
                            })),
                            3,
                        )));
                    }

                    // Home/End: ESC [ H, ESC [ F
                    b'H' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Home,
                            })),
                            3,
                        )));
                    }
                    b'F' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::End,
                            })),
                            3,
                        )));
                    }

                    // Shift+Tab: ESC [ Z
                    b'Z' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::BackTab,
                            })),
                            3,
                        )));
                    }

                    // Arrow keys with modifiers: ESC [ 1 ; modifier ch
                    b'1' if bytes.len() >= 6
                        && bytes[3] == b';'
                        && bytes[5] >= b'A'
                        && bytes[5] <= b'D' =>
                    {
                        let modifier = bytes[4] - b'0';
                        let alt = modifier & 0x2 != 0;
                        let ctrl = modifier & 0x4 != 0;

                        let code = match bytes[5] {
                            b'A' => KeyCode::Up,
                            b'B' => KeyCode::Down,
                            b'C' => KeyCode::Right,
                            b'D' => KeyCode::Left,
                            _ => {
                                // Unknown sequence, discard these bytes
                                return Ok(Some((None, 6)));
                            }
                        };

                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput { ctrl, alt, code })),
                            6,
                        )));
                    }

                    // Multi-byte sequences for special keys
                    b'1' | b'2' | b'3' | b'4' | b'5' | b'6' => {
                        if bytes.len() < 4 {
                            return Ok(None); // Need more bytes
                        }

                        if bytes[3] == b'~' {
                            let code = match bytes[2] {
                                b'1' | b'7' => KeyCode::Home, // Home
                                b'2' => KeyCode::Insert,      // Insert
                                b'3' => KeyCode::Delete,      // Delete
                                b'4' | b'8' => KeyCode::End,  // End
                                b'5' => KeyCode::PageUp,      // Page Up
                                b'6' => KeyCode::PageDown,    // Page Down
                                _ => {
                                    // Unknown sequence, discard these bytes
                                    return Ok(Some((None, 4)));
                                }
                            };
                            return Ok(Some((
                                Some(TerminalInput::Key(KeyInput {
                                    ctrl: false,
                                    alt: false,
                                    code,
                                })),
                                4,
                            )));
                        }

                        // Handle modifiers in sequences like ESC [ 1 ; 5 ~
                        if bytes.len() >= 6 && bytes[3] == b';' && bytes[5] == b'~' {
                            let code = match bytes[2] {
                                b'1' | b'7' => KeyCode::Home,
                                b'2' => KeyCode::Insert,
                                b'3' => KeyCode::Delete,
                                b'4' | b'8' => KeyCode::End,
                                b'5' => KeyCode::PageUp,
                                b'6' => KeyCode::PageDown,
                                _ => {
                                    // Unknown sequence, discard these bytes
                                    return Ok(Some((None, 6)));
                                }
                            };

                            // Parse modifier
                            let modifier = bytes[4] - b'0';
                            let alt = modifier & 0x2 != 0;
                            let ctrl = modifier & 0x4 != 0;

                            return Ok(Some((
                                Some(TerminalInput::Key(KeyInput { ctrl, alt, code })),
                                6,
                            )));
                        }

                        // Not enough bytes yet for the full sequence
                        if bytes.len() < 6 {
                            return Ok(None);
                        }

                        // Unknown sequence, discard the bytes we've examined so far
                        return Ok(Some((None, 3)));
                    }

                    _ => {
                        // Unknown escape sequence, discard the first 3 bytes
                        if bytes.len() >= 3 {
                            return Ok(Some((None, 3)));
                        }
                        return Ok(None); // Need more bytes
                    }
                }
            }

            // ESC O sequences (function keys on some terminals)
            if bytes[1] == b'O' {
                // Need at least 3 bytes for these sequences
                if bytes.len() < 3 {
                    return Ok(None); // Need more bytes
                }

                // Some terminals send ESC O A, ESC O B, etc. for arrow keys
                match bytes[2] {
                    b'A' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Up,
                            })),
                            3,
                        )));
                    }
                    b'B' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Down,
                            })),
                            3,
                        )));
                    }
                    b'C' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Right,
                            })),
                            3,
                        )));
                    }
                    b'D' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Left,
                            })),
                            3,
                        )));
                    }
                    b'H' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Home,
                            })),
                            3,
                        )));
                    }
                    b'F' => {
                        return Ok(Some((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::End,
                            })),
                            3,
                        )));
                    }
                    _ => return Ok(Some((None, 3))), // Unknown ESC O sequence
                }
            }

            // If we get here, it's either a standalone ESC key or an unknown sequence
            // Wait at least 50ms before treating it as a standalone ESC
            // But since we can't do timing here, we'll just interpret it as ESC if
            // it doesn't match any known start of a sequence
            return Ok(Some((
                Some(TerminalInput::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::Escape,
                })),
                1,
            )));
        }

        // Backspace
        0x7F => {
            return Ok(Some((
                Some(TerminalInput::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::Backspace,
                })),
                1,
            )));
        }

        // Handle UTF-8 characters
        _ if bytes[0] >= 0x80 => {
            let mut width = 1;
            if bytes[0] & 0xE0 == 0xC0 {
                width = 2;
            } else if bytes[0] & 0xF0 == 0xE0 {
                width = 3;
            } else if bytes[0] & 0xF8 == 0xF0 {
                width = 4;
            }

            if bytes.len() < width {
                return Ok(None); // Not enough bytes yet
            }

            if let Ok(s) = std::str::from_utf8(&bytes[0..width]) {
                if let Some(c) = s.chars().next() {
                    return Ok(Some((
                        Some(TerminalInput::Key(KeyInput {
                            ctrl: false,
                            alt: false,
                            code: KeyCode::Char(c),
                        })),
                        width,
                    )));
                }
            }

            // Invalid UTF-8 sequence, discard the first byte
            return Ok(Some((None, 1)));
        }

        _ => {
            // Unknown byte, discard it
            return Ok(Some((None, 1)));
        }
    }
}
