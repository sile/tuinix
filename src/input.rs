use std::io::Read;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalInput {
    Key(KeyInput),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyInput {
    pub ctrl: bool,
    pub alt: bool,
    pub code: KeyCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    Enter,
    Escape,
    Backspace,
    Tab,
    BackTab,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
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
        let Some((input, consumed_size)) = self.parse_input(&self.buf[..size])? else {
            return Ok(None);
        };
        self.buf.copy_within(consumed_size..size, 0);
        self.buf_offset = 0;
        Ok(Some(input))
    }

    fn parse_input(&self, bytes: &[u8]) -> std::io::Result<Option<(TerminalInput, usize)>> {
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
                    TerminalInput::Key(KeyInput {
                        ctrl,
                        alt: false,
                        code,
                    }),
                    1,
                )));
            }

            // Regular ASCII characters
            return Ok(Some((
                TerminalInput::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::Char(bytes[0] as char),
                }),
                1,
            )));
        }

        // Special keys and escape sequences
        match bytes[0] {
            // Escape key pressed alone
            0x1b if bytes.len() == 1 => {
                return Ok(Some((
                    TerminalInput::Key(KeyInput {
                        ctrl: false,
                        alt: false,
                        code: KeyCode::Escape,
                    }),
                    1,
                )));
            }

            // Alt + character
            0x1b if bytes.len() >= 2 && bytes[1] < 0x80 && bytes[1] != 0x1b && bytes[1] != 0x5b => {
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
                    TerminalInput::Key(KeyInput {
                        ctrl: bytes[1] < 0x20,
                        alt: true,
                        code,
                    }),
                    2,
                )));
            }

            // Escape sequences starting with ESC [
            0x1b if bytes.len() >= 3 && bytes[1] == b'[' => {
                match bytes[2] {
                    // Arrow keys: ESC [ A, ESC [ B, ESC [ C, ESC [ D
                    b'A' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Up,
                            }),
                            3,
                        )));
                    }
                    b'B' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Down,
                            }),
                            3,
                        )));
                    }
                    b'C' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Right,
                            }),
                            3,
                        )));
                    }
                    b'D' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Left,
                            }),
                            3,
                        )));
                    }

                    // Home/End: ESC [ H, ESC [ F
                    b'H' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Home,
                            }),
                            3,
                        )));
                    }
                    b'F' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::End,
                            }),
                            3,
                        )));
                    }

                    // Multi-byte sequences for special keys
                    b'1' | b'2' | b'3' | b'4' | b'5' | b'6' if bytes.len() >= 4 => {
                        if bytes[3] == b'~' {
                            let code = match bytes[2] {
                                b'1' | b'7' => KeyCode::Home, // Home
                                b'2' => KeyCode::Insert,      // Insert
                                b'3' => KeyCode::Delete,      // Delete
                                b'4' | b'8' => KeyCode::End,  // End
                                b'5' => KeyCode::PageUp,      // Page Up
                                b'6' => KeyCode::PageDown,    // Page Down
                                _ => return Ok(None),         // Unknown sequence
                            };
                            return Ok(Some((
                                TerminalInput::Key(KeyInput {
                                    ctrl: false,
                                    alt: false,
                                    code,
                                }),
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
                                _ => return Ok(None),
                            };

                            // Parse modifier
                            let modifier = bytes[4] - b'0';
                            let alt = modifier & 0x2 != 0;
                            let ctrl = modifier & 0x4 != 0;

                            return Ok(Some((TerminalInput::Key(KeyInput { ctrl, alt, code }), 6)));
                        }
                    }

                    // Shift+Tab
                    b'Z' => {
                        return Ok(Some((
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::BackTab,
                            }),
                            3,
                        )));
                    }

                    _ => {}
                }

                // Try to find escape sequences for arrow keys with modifiers
                if bytes.len() >= 6
                    && bytes[2] == b'1'
                    && bytes[3] == b';'
                    && bytes.len() >= 6
                    && bytes[5] >= b'A'
                    && bytes[5] <= b'D'
                {
                    let modifier = bytes[4] - b'0';
                    let alt = modifier & 0x2 != 0;
                    let ctrl = modifier & 0x4 != 0;

                    let code = match bytes[5] {
                        b'A' => KeyCode::Up,
                        b'B' => KeyCode::Down,
                        b'C' => KeyCode::Right,
                        b'D' => KeyCode::Left,
                        _ => return Ok(None),
                    };

                    return Ok(Some((TerminalInput::Key(KeyInput { ctrl, alt, code }), 6)));
                }
            }

            // Backspace
            0x7F => {
                return Ok(Some((
                    TerminalInput::Key(KeyInput {
                        ctrl: false,
                        alt: false,
                        code: KeyCode::Backspace,
                    }),
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
                            TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Char(c),
                            }),
                            width,
                        )));
                    }
                }
            }

            _ => {}
        }

        // If we get here, we don't recognize the sequence
        Ok(None)
    }
}
