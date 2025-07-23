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
        if self.buf_offset > 0 {
            if let Some(input) = self.read_input_from_buf()? {
                return Ok(Some(input));
            }
        }

        let read_size = self.inner.read(&mut self.buf[self.buf_offset..])?;
        if read_size == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }

        self.buf_offset += read_size;
        self.read_input_from_buf()
    }

    pub(crate) fn read_input_from_buf(&mut self) -> std::io::Result<Option<TerminalInput>> {
        let (input, consumed_size) = parse_input(&self.buf[..self.buf_offset])?;
        self.buf.copy_within(consumed_size..self.buf_offset, 0);
        self.buf_offset -= consumed_size;
        Ok(input)
    }
}

fn parse_input(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    if bytes.is_empty() {
        return Ok((None, 0)); // No bytes to parse, consumed 0 bytes
    }

    // Regular ASCII character
    if bytes[0] < 0x80 && bytes[0] != 0x1b && bytes[0] != 0x7f {
        // Control characters (Ctrl+A through Ctrl+Z)
        if bytes[0] < 0x20 {
            let (ctrl, code) = match bytes[0] {
                0x0D => (false, KeyCode::Enter), // Enter
                0x09 => (false, KeyCode::Tab),   // Tab
                c => (true, KeyCode::Char((c + 0x60) as char)),
            };
            return Ok((
                Some(TerminalInput::Key(KeyInput {
                    ctrl,
                    alt: false,
                    code,
                })),
                1,
            ));
        }

        // Regular ASCII characters
        return Ok((
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char(bytes[0] as char),
            })),
            1,
        ));
    }

    // Special keys and escape sequences
    match bytes[0] {
        // Escape key or start of escape sequence
        0x1b => {
            // For a standalone ESC press, we need to wait and see if more bytes follow
            if bytes.len() == 1 {
                return Ok((None, 0)); // Need more bytes, consumed 0 bytes
            }

            // Alt + character (ESC followed by a regular character)
            if bytes[1] < 0x80 && bytes[1] != 0x1b && bytes[1] != 0x5b && bytes[1] != 0x4f {
                let c = bytes[1] as char;
                let (ctrl, code) = if bytes[1] < 0x20 {
                    // Control characters with Alt
                    match bytes[1] {
                        0x0D => (false, KeyCode::Enter),
                        0x09 => (false, KeyCode::Tab),
                        0x08 => (false, KeyCode::Backspace),
                        c => (true, KeyCode::Char((c + 0x60) as char)),
                    }
                } else {
                    (false, KeyCode::Char(c))
                };

                return Ok((
                    Some(TerminalInput::Key(KeyInput {
                        ctrl,
                        alt: true,
                        code,
                    })),
                    2,
                ));
            }

            // ESC [ sequences (most function keys, arrow keys, etc.)
            if bytes[1] == b'[' {
                // Need at least 3 bytes for the basic arrow keys (ESC [ A)
                if bytes.len() < 3 {
                    return Ok((None, 0)); // Need more bytes, consumed 0 bytes
                }

                // Arrow keys: ESC [ A, ESC [ B, ESC [ C, ESC [ D
                match bytes[2] {
                    b'A' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Up,
                            })),
                            3,
                        ));
                    }
                    b'B' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Down,
                            })),
                            3,
                        ));
                    }
                    b'C' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Right,
                            })),
                            3,
                        ));
                    }
                    b'D' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Left,
                            })),
                            3,
                        ));
                    }

                    // Home/End: ESC [ H, ESC [ F
                    b'H' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Home,
                            })),
                            3,
                        ));
                    }
                    b'F' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::End,
                            })),
                            3,
                        ));
                    }

                    // Shift+Tab: ESC [ Z
                    b'Z' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::BackTab,
                            })),
                            3,
                        ));
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
                                return Ok((None, 6));
                            }
                        };

                        return Ok((Some(TerminalInput::Key(KeyInput { ctrl, alt, code })), 6));
                    }

                    // Multi-byte sequences for special keys
                    b'1' | b'2' | b'3' | b'4' | b'5' | b'6' => {
                        if bytes.len() < 4 {
                            return Ok((None, 0)); // Need more bytes, consumed 0 bytes
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
                                    return Ok((None, 4));
                                }
                            };
                            return Ok((
                                Some(TerminalInput::Key(KeyInput {
                                    ctrl: false,
                                    alt: false,
                                    code,
                                })),
                                4,
                            ));
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
                                    return Ok((None, 6));
                                }
                            };

                            // Parse modifier
                            let modifier = bytes[4] - b'0';
                            let alt = modifier & 0x2 != 0;
                            let ctrl = modifier & 0x4 != 0;

                            return Ok((Some(TerminalInput::Key(KeyInput { ctrl, alt, code })), 6));
                        }

                        // Not enough bytes yet for the full sequence
                        if bytes.len() < 6 {
                            return Ok((None, 0)); // Need more bytes, consumed 0 bytes
                        }

                        // Unknown sequence, discard the bytes we've examined so far
                        return Ok((None, 3));
                    }

                    _ => {
                        // Unknown escape sequence, discard the first 3 bytes
                        if bytes.len() >= 3 {
                            return Ok((None, 3));
                        }
                        return Ok((None, 0)); // Need more bytes, consumed 0 bytes
                    }
                }
            }

            // ESC O sequences (function keys on some terminals)
            if bytes[1] == b'O' {
                // Need at least 3 bytes for these sequences
                if bytes.len() < 3 {
                    return Ok((None, 0)); // Need more bytes, consumed 0 bytes
                }

                // Some terminals send ESC O A, ESC O B, etc. for arrow keys
                match bytes[2] {
                    b'A' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Up,
                            })),
                            3,
                        ));
                    }
                    b'B' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Down,
                            })),
                            3,
                        ));
                    }
                    b'C' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Right,
                            })),
                            3,
                        ));
                    }
                    b'D' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Left,
                            })),
                            3,
                        ));
                    }
                    b'H' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::Home,
                            })),
                            3,
                        ));
                    }
                    b'F' => {
                        return Ok((
                            Some(TerminalInput::Key(KeyInput {
                                ctrl: false,
                                alt: false,
                                code: KeyCode::End,
                            })),
                            3,
                        ));
                    }
                    _ => return Ok((None, 3)), // Unknown ESC O sequence
                }
            }

            // If we get here, it's either a standalone ESC key or an unknown sequence
            // Wait at least 50ms before treating it as a standalone ESC
            // But since we can't do timing here, we'll just interpret it as ESC if
            // it doesn't match any known start of a sequence
            Ok((
                Some(TerminalInput::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::Escape,
                })),
                1,
            ))
        }

        // Backspace
        0x7F => Ok((
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Backspace,
            })),
            1,
        )),

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
                return Ok((None, 0)); // Not enough bytes yet, consumed 0 bytes
            }

            if let Ok(s) = std::str::from_utf8(&bytes[0..width]) {
                if let Some(c) = s.chars().next() {
                    return Ok((
                        Some(TerminalInput::Key(KeyInput {
                            ctrl: false,
                            alt: false,
                            code: KeyCode::Char(c),
                        })),
                        width,
                    ));
                }
            }

            // Invalid UTF-8 sequence, discard the first byte
            Ok((None, 1))
        }

        _ => {
            // Unknown byte, discard it
            Ok((None, 1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_ascii_characters() {
        // Test regular ASCII characters
        let result = parse_input(b"a").unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"Z").unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('Z'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"5").unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('5'),
            }))
        );
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_parse_control_characters() {
        // Test Ctrl+A (0x01)
        let result = parse_input(&[0x01]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 1);

        // Test Ctrl+Z (0x1A)
        let result = parse_input(&[0x1A]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Char('z'),
            }))
        );
        assert_eq!(result.1, 1);

        // Test Enter (0x0D)
        let result = parse_input(&[0x0D]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Enter,
            }))
        );
        assert_eq!(result.1, 1);

        // Test Tab (0x09)
        let result = parse_input(&[0x09]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Tab,
            }))
        );
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_parse_backspace() {
        let result = parse_input(&[0x7F]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Backspace,
            }))
        );
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_parse_escape_key() {
        // Standalone ESC key
        let result = parse_input(&[0x1b]).unwrap();
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // ESC followed by unknown character should be treated as ESC
        let result = parse_input(&[0x1b, b'x']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Char('x'),
            }))
        );
        assert_eq!(result.1, 2);
    }

    #[test]
    fn test_parse_alt_combinations() {
        // Alt+a
        let result = parse_input(&[0x1b, b'a']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 2);

        // Alt+Enter
        let result = parse_input(&[0x1b, 0x0D]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Enter,
            }))
        );
        assert_eq!(result.1, 2);

        // Alt+Tab
        let result = parse_input(&[0x1b, 0x09]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Tab,
            }))
        );
        assert_eq!(result.1, 2);
    }

    #[test]
    fn test_parse_arrow_keys_esc_bracket() {
        // Up arrow: ESC [ A
        let result = parse_input(&[0x1b, b'[', b'A']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 3);

        // Down arrow: ESC [ B
        let result = parse_input(&[0x1b, b'[', b'B']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Down,
            }))
        );
        assert_eq!(result.1, 3);

        // Right arrow: ESC [ C
        let result = parse_input(&[0x1b, b'[', b'C']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Right,
            }))
        );
        assert_eq!(result.1, 3);

        // Left arrow: ESC [ D
        let result = parse_input(&[0x1b, b'[', b'D']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Left,
            }))
        );
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_parse_arrow_keys_esc_o() {
        // Up arrow: ESC O A
        let result = parse_input(&[0x1b, b'O', b'A']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 3);

        // Down arrow: ESC O B
        let result = parse_input(&[0x1b, b'O', b'B']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Down,
            }))
        );
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_parse_home_end_keys() {
        // Home: ESC [ H
        let result = parse_input(&[0x1b, b'[', b'H']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Home,
            }))
        );
        assert_eq!(result.1, 3);

        // End: ESC [ F
        let result = parse_input(&[0x1b, b'[', b'F']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::End,
            }))
        );
        assert_eq!(result.1, 3);

        // Home: ESC O H
        let result = parse_input(&[0x1b, b'O', b'H']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Home,
            }))
        );
        assert_eq!(result.1, 3);

        // End: ESC O F
        let result = parse_input(&[0x1b, b'O', b'F']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::End,
            }))
        );
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_parse_special_keys() {
        // Shift+Tab: ESC [ Z
        let result = parse_input(&[0x1b, b'[', b'Z']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::BackTab,
            }))
        );
        assert_eq!(result.1, 3);

        // Insert: ESC [ 2 ~
        let result = parse_input(&[0x1b, b'[', b'2', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Insert,
            }))
        );
        assert_eq!(result.1, 4);

        // Delete: ESC [ 3 ~
        let result = parse_input(&[0x1b, b'[', b'3', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Delete,
            }))
        );
        assert_eq!(result.1, 4);

        // Page Up: ESC [ 5 ~
        let result = parse_input(&[0x1b, b'[', b'5', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::PageUp,
            }))
        );
        assert_eq!(result.1, 4);

        // Page Down: ESC [ 6 ~
        let result = parse_input(&[0x1b, b'[', b'6', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::PageDown,
            }))
        );
        assert_eq!(result.1, 4);
    }

    #[test]
    fn test_parse_modified_arrow_keys() {
        // Ctrl+Up: ESC [ 1 ; 5 A (modifier 5 = Ctrl)
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'5', b'A']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 6);

        // Alt+Right: ESC [ 1 ; 3 C (modifier 3 = Alt)
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'C']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Right,
            }))
        );
        assert_eq!(result.1, 6);

        // Ctrl+Alt+Left: ESC [ 1 ; 7 D (modifier 7 = Ctrl+Alt)
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'7', b'D']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: true,
                alt: true,
                code: KeyCode::Left,
            }))
        );
        assert_eq!(result.1, 6);
    }

    #[test]
    fn test_parse_modified_special_keys() {
        // Ctrl+Delete: ESC [ 3 ; 5 ~
        let result = parse_input(&[0x1b, b'[', b'3', b';', b'5', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Delete,
            }))
        );
        assert_eq!(result.1, 6);

        // Alt+Home: ESC [ 1 ; 3 ~
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'~']).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Home,
            }))
        );
        assert_eq!(result.1, 6);
    }

    #[test]
    fn test_parse_utf8_characters() {
        // Test UTF-8 character (é = 0xC3 0xA9)
        let result = parse_input(&[0xC3, 0xA9]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('é'),
            }))
        );
        assert_eq!(result.1, 2);

        // Test 3-byte UTF-8 character (€ = 0xE2 0x82 0xAC)
        let result = parse_input(&[0xE2, 0x82, 0xAC]).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('€'),
            }))
        );
        assert_eq!(result.1, 3);

        // Test incomplete UTF-8 sequence
        let result = parse_input(&[0xC3]).unwrap();
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_incomplete_sequences() {
        // Incomplete escape sequence
        let result = parse_input(&[0x1b, b'[']).unwrap();
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete special key sequence
        let result = parse_input(&[0x1b, b'[', b'2']).unwrap();
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete modified key sequence
        let result = parse_input(&[0x1b, b'[', b'1', b';']).unwrap();
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_input(&[]).unwrap();
        assert_eq!(result.0, None);
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_unknown_sequences() {
        // Unknown escape sequence should be discarded
        let result = parse_input(&[0x1b, b'[', b'X']).unwrap();
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Unknown ESC O sequence
        let result = parse_input(&[0x1b, b'O', b'X']).unwrap();
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Invalid UTF-8 sequence
        let result = parse_input(&[0xFF]).unwrap();
        assert_eq!(result.0, None);
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_input_reader() {
        use std::io::Cursor;

        // Test reading a simple character
        let mut reader = InputReader::new(Cursor::new(b"a"));
        let result = reader.read_input().unwrap();
        assert_eq!(
            result,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );

        // Test reading an arrow key
        let mut reader = InputReader::new(Cursor::new(&[0x1b, b'[', b'A'][..]));
        let result = reader.read_input().unwrap();
        assert_eq!(
            result,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );

        // Test reading multiple inputs
        let mut reader = InputReader::new(Cursor::new(b"ab"));
        let result1 = reader.read_input().unwrap();
        let result2 = reader.read_input().unwrap();

        assert_eq!(
            result1,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(
            result2,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('b'),
            }))
        );
    }
}
