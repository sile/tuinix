use std::io::Read;

use crate::TerminalPosition;

/// User input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalInput {
    /// Keyboard input.
    Key(KeyInput),

    /// Mouse input.
    Mouse(MouseInput),
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

/// Mouse input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MouseInput {
    /// The type of mouse event that occurred.
    pub event: MouseEvent,

    /// The position where the mouse event occurred.
    pub position: TerminalPosition,

    /// Indicates whether the Ctrl modifier key was pressed during the event.
    pub ctrl: bool,

    /// Indicates whether the Alt modifier key was pressed during the event.
    pub alt: bool,

    /// Indicates whether the Shift modifier key was pressed during the event.
    pub shift: bool,
}

/// Mouse event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MouseEvent {
    /// Left mouse button pressed.
    LeftPress,
    /// Left mouse button released.
    LeftRelease,
    /// Right mouse button pressed.
    RightPress,
    /// Right mouse button released.
    RightRelease,
    /// Middle mouse button pressed.
    MiddlePress,
    /// Middle mouse button released.
    MiddleRelease,
    /// Mouse moved while a button is held down (drag).
    Drag,
    /// Mouse wheel scrolled up.
    ScrollUp,
    /// Mouse wheel scrolled down.
    ScrollDown,
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
        if self.buf_offset > 0
            && let Some(input) = self.read_input_from_buf()?
        {
            return Ok(Some(input));
        }

        let read_size = self.inner.read(&mut self.buf[self.buf_offset..])?;
        if read_size == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }

        self.buf_offset += read_size;
        self.read_input_from_buf()
    }

    pub(crate) fn read_input_from_buf(&mut self) -> std::io::Result<Option<TerminalInput>> {
        loop {
            let (input, consumed_size) = parse_input(&self.buf[..self.buf_offset])?;
            self.buf.copy_within(consumed_size..self.buf_offset, 0);
            self.buf_offset -= consumed_size;
            if input == None && consumed_size > 0 {
                continue;
            }
            return Ok(input);
        }
    }
}

fn parse_input(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    if bytes.is_empty() {
        return Ok((None, 0));
    }

    match bytes[0] {
        // Regular ASCII character (not escape or backspace)
        b if b < 0x80 && b != 0x1b && b != 0x7f => parse_ascii_char(bytes),
        // Escape key or escape sequence
        0x1b => parse_escape_sequence(bytes),
        // Backspace
        0x7f => Ok((Some(create_key_input(false, false, KeyCode::Backspace)), 1)),
        // UTF-8 characters
        b if b >= 0x80 => parse_utf8_char(bytes),
        // Unknown byte
        _ => Ok((None, 1)),
    }
}

fn parse_ascii_char(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let byte = bytes[0];

    // Control characters (Ctrl+A through Ctrl+Z)
    if byte < 0x20 {
        let (ctrl, code) = match byte {
            0x0D => (false, KeyCode::Enter), // Enter
            0x09 => (false, KeyCode::Tab),   // Tab
            c => (true, KeyCode::Char((c + 0x60) as char)),
        };
        return Ok((Some(create_key_input(ctrl, false, code)), 1));
    }

    // Regular ASCII characters
    Ok((
        Some(create_key_input(false, false, KeyCode::Char(byte as char))),
        1,
    ))
}

fn parse_escape_sequence(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    // Need at least 2 bytes for escape sequences
    if bytes.len() == 1 {
        return Ok((None, 0));
    }

    match bytes[1] {
        b'[' => parse_csi_sequence(bytes),
        b'O' => parse_ss3_sequence(bytes),
        // Alt + character (ESC followed by a regular character)
        b if b < 0x80 && b != 0x1b && b != 0x5b && b != 0x4f => parse_alt_char(bytes),
        // Standalone ESC or unknown sequence
        _ => Ok((Some(create_key_input(false, false, KeyCode::Escape)), 1)),
    }
}

fn parse_alt_char(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
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

    Ok((Some(create_key_input(ctrl, true, code)), 2))
}

fn parse_csi_sequence(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    // Need at least 3 bytes for basic CSI sequences (ESC [ X)
    if bytes.len() < 3 {
        return Ok((None, 0));
    }

    match bytes[2] {
        b'<' => parse_sgr_mouse_sequence(bytes),
        b'M' => parse_x10_mouse_sequence(bytes),
        b'A'..=b'D' | b'H' | b'F' | b'Z' => parse_simple_csi_key(bytes),
        b'1'..=b'6' => parse_complex_csi_key(bytes),
        _ => Ok((None, 3)), // Unknown CSI sequence
    }
}

fn parse_ss3_sequence(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    // Need at least 3 bytes for SS3 sequences (ESC O X)
    if bytes.len() < 3 {
        return Ok((None, 0));
    }

    let code = match bytes[2] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        _ => return Ok((None, 3)), // Unknown SS3 sequence
    };

    Ok((Some(create_key_input(false, false, code)), 3))
}

fn parse_simple_csi_key(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let code = match bytes[2] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => KeyCode::BackTab,
        _ => return Ok((None, 3)),
    };

    Ok((Some(create_key_input(false, false, code)), 3))
}

fn parse_complex_csi_key(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    // Handle sequences like ESC [ 1 ; 5 A (modified arrow keys)
    if bytes.len() >= 6 && bytes[2] == b'1' && bytes[3] == b';' && matches!(bytes[5], b'A'..=b'D') {
        return parse_modified_arrow_key(bytes);
    }

    // Handle sequences like ESC [ 3 ~ (Delete) or ESC [ 3 ; 5 ~ (Ctrl+Delete)
    if bytes.len() >= 4 && bytes[3] == b'~' {
        return parse_special_key_simple(bytes);
    }

    if bytes.len() >= 6 && bytes[3] == b';' && bytes[5] == b'~' {
        return parse_special_key_with_modifier(bytes);
    }

    // Need more bytes or unknown sequence
    if bytes.len() < 6 {
        Ok((None, 0))
    } else {
        Ok((None, 3))
    }
}

fn parse_modified_arrow_key(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let modifier = bytes[4] - b'0';
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    let code = match bytes[5] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        _ => return Ok((None, 6)),
    };

    Ok((Some(create_key_input(ctrl, alt, code)), 6))
}

fn parse_special_key_simple(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let code = match bytes[2] {
        b'1' | b'7' => KeyCode::Home,
        b'2' => KeyCode::Insert,
        b'3' => KeyCode::Delete,
        b'4' | b'8' => KeyCode::End,
        b'5' => KeyCode::PageUp,
        b'6' => KeyCode::PageDown,
        _ => return Ok((None, 4)),
    };

    Ok((Some(create_key_input(false, false, code)), 4))
}

fn parse_special_key_with_modifier(
    bytes: &[u8],
) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let code = match bytes[2] {
        b'1' | b'7' => KeyCode::Home,
        b'2' => KeyCode::Insert,
        b'3' => KeyCode::Delete,
        b'4' | b'8' => KeyCode::End,
        b'5' => KeyCode::PageUp,
        b'6' => KeyCode::PageDown,
        _ => return Ok((None, 6)),
    };

    let modifier = bytes[4] - b'0';
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    Ok((Some(create_key_input(ctrl, alt, code)), 6))
}

fn parse_sgr_mouse_sequence(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    // Find the end of the sequence (M or m)
    let mut end_pos = None;
    for (i, &b) in bytes.iter().enumerate().skip(3) {
        if b == b'M' || b == b'm' {
            end_pos = Some(i);
            break;
        }
    }

    let end = match end_pos {
        Some(pos) => pos,
        None => return Ok((None, 0)), // Incomplete sequence
    };

    // Parse the parameters
    let params_str = std::str::from_utf8(&bytes[3..end])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))?;

    let params: Vec<&str> = params_str.split(';').collect();
    if params.len() != 3 {
        return Ok((None, end + 1)); // Invalid parameter count
    }

    let (button, x, y) = match (
        params[0].parse::<u16>(),
        params[1].parse::<u16>(),
        params[2].parse::<u16>(),
    ) {
        (Ok(b), Ok(x), Ok(y)) => (b, x, y),
        _ => return Ok((None, end + 1)), // Invalid parameters
    };

    let mouse_input = create_sgr_mouse_input(button, x, y, bytes[end] == b'm')?;
    match mouse_input {
        Some(input) => Ok((Some(TerminalInput::Mouse(input)), end + 1)),
        None => Ok((None, end + 1)),
    }
}

fn parse_x10_mouse_sequence(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    if bytes.len() < 6 {
        return Ok((None, 0));
    }

    let button_byte = bytes[3];
    let x = bytes[4] as u16;
    let y = bytes[5] as u16;

    let mouse_input = create_x10_mouse_input(button_byte, x, y);
    Ok((Some(TerminalInput::Mouse(mouse_input)), 6))
}

fn parse_utf8_char(bytes: &[u8]) -> std::io::Result<(Option<TerminalInput>, usize)> {
    let width = match bytes[0] {
        b if b & 0xE0 == 0xC0 => 2,
        b if b & 0xF0 == 0xE0 => 3,
        b if b & 0xF8 == 0xF0 => 4,
        _ => 1,
    };

    if bytes.len() < width {
        return Ok((None, 0)); // Not enough bytes yet
    }

    match std::str::from_utf8(&bytes[0..width]) {
        Ok(s) => {
            if let Some(c) = s.chars().next() {
                Ok((
                    Some(create_key_input(false, false, KeyCode::Char(c))),
                    width,
                ))
            } else {
                Ok((None, 1)) // Invalid UTF-8, discard first byte
            }
        }
        Err(_) => Ok((None, 1)), // Invalid UTF-8, discard first byte
    }
}

// Helper functions
fn create_key_input(ctrl: bool, alt: bool, code: KeyCode) -> TerminalInput {
    TerminalInput::Key(KeyInput { ctrl, alt, code })
}

fn create_sgr_mouse_input(
    button: u16,
    x: u16,
    y: u16,
    is_release: bool,
) -> std::io::Result<Option<MouseInput>> {
    let button_code = button & 0x03;
    let ctrl = (button & 0x10) != 0;
    let alt = (button & 0x08) != 0;
    let shift = (button & 0x04) != 0;
    let drag = (button & 0x20) != 0;

    let event = if drag {
        MouseEvent::Drag
    } else if is_release {
        match button_code {
            0 => MouseEvent::LeftRelease,
            1 => MouseEvent::MiddleRelease,
            2 => MouseEvent::RightRelease,
            _ => return Ok(None),
        }
    } else {
        // Check for scroll events first
        match button {
            64 => MouseEvent::ScrollUp,
            65 => MouseEvent::ScrollDown,
            _ => match button_code {
                0 => MouseEvent::LeftPress,
                1 => MouseEvent::MiddlePress,
                2 => MouseEvent::RightPress,
                _ => return Ok(None),
            },
        }
    };

    Ok(Some(MouseInput {
        event,
        position: TerminalPosition::row_col(
            y.saturating_sub(1) as usize,
            x.saturating_sub(1) as usize,
        ),
        ctrl,
        alt,
        shift,
    }))
}

fn create_x10_mouse_input(button_byte: u8, x: u16, y: u16) -> MouseInput {
    let ctrl = (button_byte & 0x10) != 0;
    let alt = (button_byte & 0x08) != 0;
    let shift = (button_byte & 0x04) != 0;

    let event = match button_byte {
        96 => MouseEvent::ScrollUp,
        97 => MouseEvent::ScrollDown,
        _ => {
            // Remove modifier bits to get the base button code
            let base_button = button_byte & !0x1C; // Remove shift(4), alt(8), ctrl(16) bits

            match base_button {
                32 => MouseEvent::LeftPress,   // 0x20
                33 => MouseEvent::MiddlePress, // 0x21
                34 => MouseEvent::RightPress,  // 0x22
                35 => MouseEvent::LeftRelease, // 0x23
                64 => MouseEvent::Drag,        // 0x40
                _ => {
                    // Fallback: check bottom 2 bits for button type
                    match button_byte & 0x03 {
                        0 => MouseEvent::LeftPress,
                        1 => MouseEvent::MiddlePress,
                        2 => MouseEvent::RightPress,
                        3 => MouseEvent::LeftRelease,
                        _ => MouseEvent::LeftPress,
                    }
                }
            }
        }
    };

    MouseInput {
        event,
        position: TerminalPosition::row_col(
            y.saturating_sub(33) as usize,
            x.saturating_sub(33) as usize,
        ),
        ctrl,
        alt,
        shift,
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

    #[test]
    fn test_parse_mouse_scroll_events() {
        // SGR mode scroll up: ESC [ < 64 ; 10 ; 5 M
        let input = b"\x1b[<64;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::ScrollUp,
                position: TerminalPosition::row_col(4, 9), // row: 5-1, col: 10-1
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode scroll down: ESC [ < 65 ; 10 ; 5 M
        let input = b"\x1b[<65;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::ScrollDown,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_sgr_mode_button_press() {
        // SGR mode left button press: ESC [ < 0 ; 10 ; 5 M
        let input = b"\x1b[<0;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9), // row: 5-1, col: 10-1
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(result.1, input.len());

        // SGR mode middle button press: ESC [ < 1 ; 10 ; 5 M
        let input = b"\x1b[<1;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::MiddlePress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode right button press: ESC [ < 2 ; 10 ; 5 M
        let input = b"\x1b[<2;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::RightPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_sgr_mode_button_release() {
        // SGR mode left button release: ESC [ < 0 ; 10 ; 5 m (lowercase 'm')
        let input = b"\x1b[<0;10;5m";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftRelease,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode middle button release: ESC [ < 1 ; 10 ; 5 m
        let input = b"\x1b[<1;10;5m";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::MiddleRelease,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode right button release: ESC [ < 2 ; 10 ; 5 m
        let input = b"\x1b[<2;10;5m";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::RightRelease,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_sgr_mode_with_modifiers() {
        // SGR mode with Ctrl modifier: ESC [ < 16 ; 10 ; 5 M (16 = 0 + 16)
        let input = b"\x1b[<16;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: true,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode with Alt modifier: ESC [ < 8 ; 10 ; 5 M (8 = 0 + 8)
        let input = b"\x1b[<8;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: true,
                shift: false,
            }))
        );

        // SGR mode with Shift modifier: ESC [ < 4 ; 10 ; 5 M (4 = 0 + 4)
        let input = b"\x1b[<4;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: true,
            }))
        );

        // SGR mode with all modifiers: ESC [ < 28 ; 10 ; 5 M (28 = 0 + 4 + 8 + 16)
        let input = b"\x1b[<28;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: true,
                alt: true,
                shift: true,
            }))
        );
    }
    #[test]
    fn test_parse_mouse_sgr_mode_drag() {
        // SGR mode drag: ESC [ < 32 ; 10 ; 5 M (32 = 0 + 32)
        let input = b"\x1b[<32;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::Drag,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode drag with modifiers: ESC [ < 60 ; 10 ; 5 M (60 = 0 + 4 + 8 + 16 + 32)
        let input = b"\x1b[<60;10;5M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::Drag,
                position: TerminalPosition::row_col(4, 9),
                ctrl: true,
                alt: true,
                shift: true,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_x10_x11_mode() {
        // X10/X11 mode left button press: ESC [ M <button> <x> <y>
        // Button 32 (0x20) = left press, x=43 (10+33), y=38 (5+33)
        let input = b"\x1b[M \x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(result.1, 6);

        // X10/X11 mode middle button press: ESC [ M <button> <x> <y>
        // Button 33 (0x21) = middle press
        let input = b"\x1b[M!\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::MiddlePress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode right button press: ESC [ M <button> <x> <y>
        // Button 34 (0x22) = right press
        let input = b"\x1b[M\"\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::RightPress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode button release: ESC [ M <button> <x> <y>
        // Button 35 (0x23) = release
        let input = b"\x1b[M#\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftRelease,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_x10_x11_mode_with_modifiers() {
        // X10/X11 mode with Ctrl modifier: button = 32 + 16 = 48 (0x30)
        let input = b"\x1b[M0\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: true,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode with Alt modifier: button = 32 + 8 = 40 (0x28)
        let input = b"\x1b[M(\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: true,
                shift: false,
            }))
        );

        // X10/X11 mode with Shift modifier: button = 32 + 4 = 36 (0x24)
        let input = b"\x1b[M$\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: true,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_x10_x11_mode_scroll() {
        // X10/X11 mode scroll up: button = 96 (0x60)
        let input = b"\x1b[M`\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::ScrollUp,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode scroll down: button = 97 (0x61)
        let input = b"\x1b[Ma\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::ScrollDown,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_x10_x11_mode_drag() {
        // X10/X11 mode drag: button = 32 + 32 = 64 (0x40)
        let input = b"\x1b[M@\x2b\x26";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::Drag,
                position: TerminalPosition::row_col(5, 10),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_coordinate_boundaries() {
        // Test coordinates at origin (1,1 -> 0,0)
        let input = b"\x1b[<0;1;1M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(0, 0),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // Test large coordinates
        let input = b"\x1b[<0;100;200M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(199, 99), // row: 200-1, col: 100-1
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_parse_mouse_edge_cases() {
        // SGR sequence with zero coordinates (should saturate to 0)
        let input = b"\x1b[<0;0;0M";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(0, 0), // saturating_sub(1) on 0 = 0
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 sequence with minimum coordinate values (33)
        let input = b"\x1b[M !!";
        let result = parse_input(input).unwrap();
        assert_eq!(
            result.0,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(0, 0), // 33-33 = 0
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_input_reader_mouse_events() {
        use std::io::Cursor;

        // Test reading a mouse click
        let mut reader = InputReader::new(Cursor::new(b"\x1b[<0;10;5M"));
        let result = reader.read_input().unwrap();
        assert_eq!(
            result,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // Test reading multiple mouse events
        let mut reader = InputReader::new(Cursor::new(b"\x1b[<0;10;5M\x1b[<0;10;5m"));
        let result1 = reader.read_input().unwrap();
        let result2 = reader.read_input().unwrap();

        assert_eq!(
            result1,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftPress,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(
            result2,
            Some(TerminalInput::Mouse(MouseInput {
                event: MouseEvent::LeftRelease,
                position: TerminalPosition::row_col(4, 9),
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }
}
