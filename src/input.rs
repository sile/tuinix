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

    pub(crate) fn replace_inner(&mut self, inner: R) {
        self.inner = inner;
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
            if input.is_none() && consumed_size > 0 {
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
        Ok(s) if let Some(c) = s.chars().next() => Ok((
            Some(create_key_input(false, false, KeyCode::Char(c))),
            width,
        )),
        _ => Ok((None, 1)), // Invalid UTF-8, discard first byte
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
    use std::{cell::Cell, io::Cursor};

    use super::*;

    #[test]
    fn test_parse_regular_ascii_characters() {
        // Test regular ASCII characters
        let result = parse_input(b"a").expect("parse succeeds");
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"Z").expect("parse succeeds");
        assert_eq!(
            result.0,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('Z'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"5").expect("parse succeeds");
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
        let result = parse_input(&[0x01]).expect("parse succeeds");
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
        let result = parse_input(&[0x1A]).expect("parse succeeds");
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
        let result = parse_input(&[0x0D]).expect("parse succeeds");
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
        let result = parse_input(&[0x09]).expect("parse succeeds");
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
        let result = parse_input(&[0x7F]).expect("parse succeeds");
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
        let result = parse_input(&[0x1b]).expect("parse succeeds");
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // ESC followed by unknown character should be treated as ESC
        let result = parse_input(&[0x1b, b'x']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'a']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, 0x0D]).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, 0x09]).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'A']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'B']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'C']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'D']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'O', b'A']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'O', b'B']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'H']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'F']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'O', b'H']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'O', b'F']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'Z']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'2', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'3', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'5', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'6', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'5', b'A']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'C']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'7', b'D']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'3', b';', b'5', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'~']).expect("parse succeeds");
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
        let result = parse_input(&[0xC3, 0xA9]).expect("parse succeeds");
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
        let result = parse_input(&[0xE2, 0x82, 0xAC]).expect("parse succeeds");
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
        let result = parse_input(&[0xC3]).expect("parse succeeds");
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_incomplete_sequences() {
        // Incomplete escape sequence
        let result = parse_input(&[0x1b, b'[']).expect("parse succeeds");
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete special key sequence
        let result = parse_input(&[0x1b, b'[', b'2']).expect("parse succeeds");
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete modified key sequence
        let result = parse_input(&[0x1b, b'[', b'1', b';']).expect("parse succeeds");
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_input(&[]).expect("parse succeeds");
        assert_eq!(result.0, None);
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_unknown_sequences() {
        // Unknown escape sequence should be discarded
        let result = parse_input(&[0x1b, b'[', b'X']).expect("parse succeeds");
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Unknown ESC O sequence
        let result = parse_input(&[0x1b, b'O', b'X']).expect("parse succeeds");
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Invalid UTF-8 sequence
        let result = parse_input(&[0xFF]).expect("parse succeeds");
        assert_eq!(result.0, None);
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_replace_inner_preserves_buffer_then_reads_new_inner() {
        let mut reader = InputReader::new(Cursor::new(&b"ab"[..]));

        // Read the first input. The remaining byte is kept in the internal buffer.
        let first = reader.read_input().expect("read succeeds");
        assert_eq!(
            first,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );

        // Replacing the inner reader must preserve the buffered data and then
        // continue reading from the new inner reader.
        reader.replace_inner(Cursor::new(&b"c"[..]));
        let second = reader.read_input().expect("read succeeds");
        assert_eq!(
            second,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('b'),
            }))
        );
        let third = reader.read_input().expect("read succeeds");
        assert_eq!(
            third,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('c'),
            }))
        );
    }

    #[test]
    fn test_replace_inner_combines_partial_sequence_with_new_inner() {
        // An incomplete escape sequence stays in the buffer.
        let mut reader = InputReader::new(Cursor::new(&[0x1b, b'['][..]));
        let none = reader.read_input().expect("read succeeds");
        assert_eq!(none, None);

        // The continuation arrives from the new inner reader.
        reader.replace_inner(Cursor::new(&b"A"[..]));
        let input = reader.read_input().expect("read succeeds");
        assert_eq!(
            input,
            Some(TerminalInput::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
    }

    #[test]
    fn test_replace_inner_propagates_new_inner_eof() {
        let mut reader = InputReader::new(Cursor::new(&b"ab"[..]));
        assert!(reader.read_input().expect("read succeeds").is_some());

        reader.replace_inner(Cursor::new(&b""[..]));
        assert!(reader.read_input().expect("read succeeds").is_some());

        // After the buffer is consumed, EOF from the new inner reader propagates.
        let err = reader.read_input().expect_err("should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn test_input_reader() {
        // Test reading a simple character
        let mut reader = InputReader::new(Cursor::new(b"a"));
        let result = reader.read_input().expect("read succeeds");
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
        let result = reader.read_input().expect("read succeeds");
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
        let result1 = reader.read_input().expect("read succeeds");
        let result2 = reader.read_input().expect("read succeeds");

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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        let result = parse_input(input).expect("parse succeeds");
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
        // Test reading a mouse click
        let mut reader = InputReader::new(Cursor::new(b"\x1b[<0;10;5M"));
        let result = reader.read_input().expect("read succeeds");
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
        let result1 = reader.read_input().expect("read succeeds");
        let result2 = reader.read_input().expect("read succeeds");

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

    // ---- PBT helpers ----

    fn sample_pbt_bytes(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
        let len =
            noprop::sample_with_boundaries(ctx, &[0usize, 64], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 0..=64)
            });
        noprop::sample_bytes_vec(ctx, len)
    }

    /// Byte-sequence fragments covering the parse paths: plain ASCII,
    /// control characters, arrow keys, special keys, modified keys,
    /// mouse sequences, UTF-8, unknown sequences, and incomplete
    /// escape sequences.
    const PBT_FRAGMENTS: &[&[u8]] = &[
        b"a",
        b"Z",
        b"5",
        b"!",
        b"\x01",
        b"\x0d",
        b"\x09",
        b"\x7f",
        b"\x1b[A",
        b"\x1b[B",
        b"\x1bOH",
        b"\x1b[Z",
        b"\x1b[2~",
        b"\x1b[1;5A",
        b"\x1b[3;5~",
        b"\x1b[<0;10;5M",
        b"\x1b[M!\x2b\x26",
        "\u{3042}".as_bytes(),
        b"\x1b[X",
        b"\x1bOX",
        b"\x1b",
        b"\x1b[",
    ];

    fn sample_pbt_fragments(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
        let n =
            noprop::sample_with_boundaries(ctx, &[1usize, 16], noprop::Ratio::one_nth(5), |ctx| {
                noprop::sample_usize_in(ctx, 1..=16)
            });
        let mut bytes = Vec::new();
        for _ in 0..n {
            bytes.extend_from_slice(noprop::sample_choice(ctx, PBT_FRAGMENTS));
        }
        bytes
    }

    const ARROW_CODES: [KeyCode; 4] = [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right];

    const SPECIAL_CODES: [KeyCode; 10] = [
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::BackTab,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Home,
        KeyCode::End,
    ];

    /// `Ctrl+X` byte values that do not collide with Backspace
    /// (0x08), Tab (0x09), or Enter (0x0d): 'h', 'i', and 'm' are
    /// excluded.
    const CTRL_CHARS: &[char] = &[
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'j', 'k', 'l', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u',
        'v', 'w', 'x', 'y', 'z',
    ];

    const MULTIBYTE_CHARS: &[char] = &['あ', '界', 'é', '€', '😀'];

    /// `Alt+Char` characters that do not collide with the CSI ('[')
    /// or SS3 ('O') sequence starts, which `parse_escape_sequence`
    /// resolves before `parse_alt_char`.
    const ALT_CHARS: &[char] = &['a', 'Z', '5', '!', '~', ' ', '@', '#', '0', 'x'];

    fn encodable_modifiers(code: KeyCode) -> &'static [(bool, bool)] {
        match code {
            KeyCode::Enter | KeyCode::Tab | KeyCode::Backspace => &[(false, false), (false, true)],
            KeyCode::BackTab => &[(false, false)],
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Delete
            | KeyCode::Insert
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => &[(false, false), (false, true), (true, false), (true, true)],
            _ => &[],
        }
    }

    fn sample_encodable_key(ctx: &mut noprop::TestCaseContext) -> KeyInput {
        match noprop::sample_weighted_index(ctx, &[3, 2, 5]) {
            0 => {
                let code = noprop::sample_choice(ctx, &SPECIAL_CODES);
                let (ctrl, alt) = noprop::sample_choice(ctx, encodable_modifiers(code));
                KeyInput { ctrl, alt, code }
            }
            1 => {
                let code = noprop::sample_choice(ctx, &ARROW_CODES);
                let (ctrl, alt) = noprop::sample_choice(
                    ctx,
                    &[(false, false), (false, true), (true, false), (true, true)],
                );
                KeyInput { ctrl, alt, code }
            }
            _ => {
                let ctrl = noprop::sample_bool(ctx);
                let alt = noprop::sample_bool(ctx);
                let code = if ctrl {
                    KeyCode::Char(noprop::sample_choice(ctx, CTRL_CHARS))
                } else if alt {
                    KeyCode::Char(noprop::sample_choice(ctx, ALT_CHARS))
                } else {
                    match noprop::sample_weighted_index(ctx, &[4, 1]) {
                        0 => KeyCode::Char(
                            char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                                .expect("valid ASCII"),
                        ),
                        _ => KeyCode::Char(noprop::sample_choice(ctx, MULTIBYTE_CHARS)),
                    }
                };
                KeyInput { ctrl, alt, code }
            }
        }
    }

    /// Encodes a `KeyInput` as the byte sequence a terminal would
    /// produce for it. Returns `None` for keys with no canonical
    /// representation.
    fn encode_key(input: KeyInput) -> Option<Vec<u8>> {
        let KeyInput { ctrl, alt, code } = input;
        let esc = |bytes: &[u8]| {
            let mut v = vec![0x1b];
            v.extend_from_slice(bytes);
            v
        };
        let modified = |params: u8, final_byte: u8| {
            let m = 1 + if alt { 2 } else { 0 } + if ctrl { 4 } else { 0 };
            vec![0x1b, b'[', params, b';', b'0' + m, final_byte]
        };
        match code {
            KeyCode::Enter if !ctrl => Some(if alt { esc(&[0x0d]) } else { vec![0x0d] }),
            KeyCode::Tab if !ctrl => Some(if alt { esc(&[0x09]) } else { vec![0x09] }),
            KeyCode::Backspace if !ctrl => Some(if alt { esc(&[0x08]) } else { vec![0x7f] }),
            KeyCode::BackTab if !ctrl && !alt => Some(vec![0x1b, b'[', b'Z']),
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right if !ctrl && !alt => {
                let dir = match code {
                    KeyCode::Up => b'A',
                    KeyCode::Down => b'B',
                    KeyCode::Left => b'D',
                    KeyCode::Right => b'C',
                    _ => unreachable!(),
                };
                Some(vec![0x1b, b'[', dir])
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                let dir = match code {
                    KeyCode::Up => b'A',
                    KeyCode::Down => b'B',
                    KeyCode::Left => b'D',
                    KeyCode::Right => b'C',
                    _ => unreachable!(),
                };
                Some(modified(b'1', dir))
            }
            KeyCode::Home | KeyCode::End if !ctrl && !alt => {
                let f = if code == KeyCode::Home { b'H' } else { b'F' };
                Some(vec![0x1b, b'[', f])
            }
            KeyCode::Home | KeyCode::End => {
                let n = if code == KeyCode::Home { b'1' } else { b'4' };
                Some(modified(n, b'~'))
            }
            KeyCode::Delete | KeyCode::Insert | KeyCode::PageUp | KeyCode::PageDown
                if !ctrl && !alt =>
            {
                let n = match code {
                    KeyCode::Insert => b'2',
                    KeyCode::Delete => b'3',
                    KeyCode::PageUp => b'5',
                    KeyCode::PageDown => b'6',
                    _ => unreachable!(),
                };
                Some(vec![0x1b, b'[', n, b'~'])
            }
            KeyCode::Delete | KeyCode::Insert | KeyCode::PageUp | KeyCode::PageDown => {
                let n = match code {
                    KeyCode::Insert => b'2',
                    KeyCode::Delete => b'3',
                    KeyCode::PageUp => b'5',
                    KeyCode::PageDown => b'6',
                    _ => unreachable!(),
                };
                Some(modified(n, b'~'))
            }
            KeyCode::Char(c) if ctrl => {
                debug_assert!(c.is_ascii_lowercase() && !matches!(c, 'h' | 'i' | 'm'));
                let b = c as u8 - 0x60;
                Some(if alt { esc(&[b]) } else { vec![b] })
            }
            KeyCode::Char(c) if alt => Some(esc(&[c as u8])),
            KeyCode::Char(c) => {
                let mut v = Vec::new();
                v.extend_from_slice(c.to_string().as_bytes());
                Some(v)
            }
            KeyCode::Escape
            | KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Backspace
            | KeyCode::BackTab => None,
        }
    }

    // ---- PBT ----

    /// `parse_input` must never fail on arbitrary byte sequences, must
    /// consume at most the input length, and must consume at least one
    /// byte whenever it reports an input.
    ///
    /// Known defect, tracked separately and not fixed here: feeding
    /// `ESC [ 1 ; ! A` (or any non-digit byte at the modifier
    /// position) panics with `attempt to subtract with overflow` in
    /// `parse_modified_arrow_key` / `parse_special_key_with_modifier`
    /// (`bytes[4] - b'0'`). Reproduced with
    /// `TUINIX_PBT_SEED=0x18cac336d9f9c4d0`, case 0. Random generation
    /// makes this input so unlikely that this test still passes; the
    /// defect should be exercised from that seed once it is fixed.
    #[test]
    fn pbt_parse_input_invariants() -> noprop::TestResult {
        let observed_partial = Cell::new(false);
        let observed_multibyte = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            // Half of the cases start with an incomplete escape
            // sequence, so the (None, 0) partial-sequence path is
            // exercised structurally instead of by chance.
            let structured = noprop::sample_bool(ctx);
            let bytes = if structured {
                match noprop::sample_usize_in(ctx, 0..3) {
                    0 => vec![0x1b],
                    1 => vec![0x1b, b'['],
                    _ => vec![0x1b, b'O'],
                }
            } else {
                sample_pbt_bytes(ctx)
            };
            let (input, consumed) = parse_input(&bytes).expect("parse_input must not fail");
            assert!(
                consumed <= bytes.len(),
                "consumed {consumed} exceeds length {}",
                bytes.len()
            );
            if input.is_some() {
                assert!(
                    consumed >= 1,
                    "a parsed input must consume at least one byte"
                );
            }
            if bytes.first().is_some_and(|b| *b >= 0x80) {
                observed_multibyte.set(true);
            }
            if input.is_none() && consumed == 0 && !bytes.is_empty() {
                observed_partial.set(true);
            }
            Ok(())
        })?;
        assert!(
            observed_multibyte.get(),
            "no case fed a multibyte lead byte\n{runner}"
        );
        assert!(
            observed_partial.get(),
            "no case observed an incomplete sequence\n{runner}"
        );
        Ok(())
    }

    /// A `KeyInput` encodable as a standard terminal byte sequence
    /// must round-trip: parsing the encoded bytes must reproduce the
    /// same key and consume the whole sequence.
    #[test]
    fn pbt_key_input_roundtrip() -> noprop::TestResult {
        let observed_ctrl = Cell::new(false);
        let observed_alt = Cell::new(false);
        let observed_multibyte = Cell::new(false);
        let observed_modified = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let key = sample_encodable_key(ctx);
            let bytes = encode_key(key).expect("generated key must be encodable");
            let (input, consumed) = parse_input(&bytes).expect("parse_input must not fail");
            assert_eq!(
                input,
                Some(TerminalInput::Key(key)),
                "round-trip mismatch: {key:?} -> {bytes:?}"
            );
            assert_eq!(
                consumed,
                bytes.len(),
                "consumed length mismatch: {key:?} -> {bytes:?}"
            );
            if key.ctrl {
                observed_ctrl.set(true);
            }
            if key.alt {
                observed_alt.set(true);
            }
            if let KeyCode::Char(c) = key.code
                && !c.is_ascii()
            {
                observed_multibyte.set(true);
            }
            if (key.ctrl || key.alt)
                && matches!(
                    key.code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Delete
                        | KeyCode::Insert
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                        | KeyCode::Home
                        | KeyCode::End
                )
            {
                observed_modified.set(true);
            }
            Ok(())
        })?;
        assert!(
            observed_ctrl.get(),
            "no case exercised the ctrl modifier\n{runner}"
        );
        assert!(
            observed_alt.get(),
            "no case exercised the alt modifier\n{runner}"
        );
        assert!(
            observed_multibyte.get(),
            "no case exercised a multibyte character\n{runner}"
        );
        assert!(
            observed_modified.get(),
            "no case exercised a modified key\n{runner}"
        );
        Ok(())
    }

    /// `read_input_from_buf` must agree with a model that applies
    /// `parse_input` repeatedly to the same bytes: the same event
    /// sequence, stopping at the same incomplete or fully consumed
    /// sequence.
    #[test]
    fn pbt_input_buffer_matches_parse_model() -> noprop::TestResult {
        let observed_event = Cell::new(false);
        let observed_partial = Cell::new(false);
        let observed_unknown = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let bytes = sample_pbt_fragments(ctx);
            let mut reader = InputReader {
                inner: Cursor::new(&[]),
                buf: bytes.clone(),
                buf_offset: bytes.len(),
            };
            let mut actual = Vec::new();
            let actual_partial;
            loop {
                match reader.read_input_from_buf() {
                    Ok(Some(input)) => actual.push(input),
                    Ok(None) => {
                        actual_partial = reader.buf_offset > 0;
                        break;
                    }
                    Err(e) => panic!("read_input_from_buf must not fail: {e}"),
                }
            }
            let mut expected = Vec::new();
            let mut expected_partial = false;
            let mut expected_unknown = false;
            let mut rest = &bytes[..];
            loop {
                if rest.is_empty() {
                    break;
                }
                let (input, consumed) = parse_input(rest).expect("parse_input must not fail");
                assert!(consumed <= rest.len(), "consumed exceeds remaining bytes");
                if consumed == 0 {
                    expected_partial = true;
                    break;
                }
                if input.is_none() {
                    expected_unknown = true;
                }
                rest = &rest[consumed..];
                if let Some(input) = input {
                    expected.push(input);
                }
            }
            assert_eq!(actual, expected, "event mismatch for {bytes:?}");
            assert_eq!(
                actual_partial, expected_partial,
                "partial-stop mismatch for {bytes:?}"
            );
            if !actual.is_empty() {
                observed_event.set(true);
            }
            if expected_partial {
                observed_partial.set(true);
            }
            if expected_unknown {
                observed_unknown.set(true);
            }
            Ok(())
        })?;
        assert!(observed_event.get(), "no case parsed any event\n{runner}");
        assert!(
            observed_partial.get(),
            "no case stopped at an incomplete sequence\n{runner}"
        );
        assert!(
            observed_unknown.get(),
            "no case consumed an unknown sequence\n{runner}"
        );
        Ok(())
    }
}
