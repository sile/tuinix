use crate::Position;

/// User input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Input {
    /// A key was pressed.
    Key(KeyInput),

    /// The mouse produced a button, drag, or wheel input.
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
    /// The Enter key.
    Enter,

    /// The Escape key.
    Escape,

    /// The Backspace key.
    Backspace,

    /// The Tab key.
    Tab,

    /// The Tab key with Shift held (backwards tab).
    BackTab,

    /// The Delete key.
    Delete,

    /// The Insert key.
    Insert,

    /// The Up arrow key.
    Up,

    /// The Down arrow key.
    Down,

    /// The Left arrow key.
    Left,

    /// The Right arrow key.
    Right,

    /// The Home key.
    Home,

    /// The End key.
    End,

    /// The Page Up key.
    PageUp,

    /// The Page Down key.
    PageDown,

    /// A character key.
    Char(char),
}

/// Mouse input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MouseInput {
    /// The kind of mouse input that occurred.
    pub kind: MouseInputKind,

    /// The position where the mouse input occurred.
    pub position: Position,

    /// Indicates whether the Ctrl modifier key was pressed for this input.
    pub ctrl: bool,

    /// Indicates whether the Alt modifier key was pressed for this input.
    pub alt: bool,

    /// Indicates whether the Shift modifier key was pressed for this input.
    pub shift: bool,
}

/// Mouse input kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MouseInputKind {
    /// The left button was pressed.
    LeftPress,

    /// The left button was released.
    LeftRelease,

    /// The right button was pressed.
    RightPress,

    /// The right button was released.
    RightRelease,

    /// The middle button was pressed.
    MiddlePress,

    /// The middle button was released.
    MiddleRelease,

    /// The mouse moved while a button was held down (drag).
    Drag,

    /// The wheel was scrolled up.
    ScrollUp,

    /// The wheel was scrolled down.
    ScrollDown,
}

/// The pure, I/O-free decoder that accumulates raw bytes until a complete
/// [`Input`] can be parsed.
///
/// It is driven by the application: it has no awareness of any `Read` source,
/// so it can live outside the driver and be fed whatever bytes the application
/// reads from a terminal or elsewhere.
///
/// Feed raw bytes with [`InputDecoder::feed()`](Self::feed) and pull parsed
/// [`Input`] values with [`InputDecoder::next()`](Self::next). A lone
/// `ESC` byte is held until it is completed by more bytes or committed as the
/// Escape key with [`InputDecoder::commit_escape()`](Self::commit_escape).
///
/// It does not bound how many bytes it holds. An application that can
/// receive unparsable input (for example a large paste) should watch
/// [`buffered_bytes()`](Self::buffered_bytes) and drop the excess with
/// [`discard_buffered_bytes()`](Self::discard_buffered_bytes).
#[derive(Debug, Default)]
pub struct InputDecoder {
    buf: Vec<u8>,
    // A lone `ESC` byte committed by `commit_escape()`, waiting for `next()` to
    // emit it. A flag is used instead of a sentinel byte because `parse_input`
    // holds a lone `ESC` back as an incomplete sequence, so no in-buffer
    // representation would be returned by `next()`.
    committed_escape: bool,
}

impl InputDecoder {
    /// Makes a new, empty input decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds raw bytes into the decoder.
    ///
    /// Unparsed bytes are held until [`next()`](Self::next) can produce an
    /// [`Input`] from them. The decoder does not bound how many bytes it holds;
    /// an application that can receive unparsable input should watch
    /// [`buffered_bytes()`](Self::buffered_bytes) and drop the excess with
    /// [`discard_buffered_bytes()`](Self::discard_buffered_bytes).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Parses and returns the next complete [`Input`], consuming its bytes.
    ///
    /// Returns `None` when no complete [`Input`] can be produced from the bytes fed
    /// so far. That is the normal outcome of an incomplete sequence: a lone
    /// `ESC` byte is held until more bytes arrive or it is committed with
    /// [`commit_escape()`](Self::commit_escape). You do not need to track the
    /// buffer yourself; read with `feed()` and drain with `next()`.
    //
    // `InputDecoder` is a stateful parser, not an iterator; the name `next` is
    // kept for symmetry with `feed`. Implementing `Iterator` would not be a
    // natural fit here: `None` means "no complete input from the bytes fed so
    // far", not "the decoder is exhausted", so the `Iterator` contract would
    // mislead a caller into reading it as end of input.
    #[expect(
        clippy::should_implement_trait,
        reason = "`InputDecoder` is a stateful parser; `next` is kept for symmetry with `feed`"
    )]
    pub fn next(&mut self) -> Option<Input> {
        if self.committed_escape {
            self.committed_escape = false;
            return Some(create_key_input(false, false, KeyCode::Escape));
        }

        loop {
            let (input, consumed) = parse_input(&self.buf);
            if consumed > 0 {
                self.buf.drain(..consumed);
            }
            if input.is_none() && consumed > 0 {
                continue;
            }
            return input;
        }
    }

    /// Returns the number of bytes buffered but not yet consumed by
    /// [`next()`](Self::next).
    ///
    /// The count includes an incomplete sequence that is being held for more
    /// bytes. Use it to bound how much memory a decoder can take: when the count
    /// grows past what the application wants to keep, drop the excess with
    /// [`discard_buffered_bytes()`](Self::discard_buffered_bytes).
    pub fn buffered_bytes(&self) -> usize {
        self.buf.len()
    }

    /// Discards up to `len` bytes from the front of the buffer and returns how
    /// many bytes were actually discarded.
    ///
    /// `len` is clipped to the number of buffered bytes, so passing a larger
    /// value discards everything and returns the buffer length. This is how an
    /// application enforces its own bound on [`buffered_bytes()`](Self::buffered_bytes):
    /// the decoder never drops bytes on its own, because only the application
    /// knows whether discarding a partial sequence is acceptable.
    pub fn discard_buffered_bytes(&mut self, len: usize) -> usize {
        let len = len.min(self.buf.len());
        self.buf.drain(..len);
        len
    }

    // Returns `true` when the decoder holds unconsumed bytes. Only the tests
    // assert on the residual buffer, so this is not part of the public surface.
    #[cfg(test)]
    fn has_pending(&self) -> bool {
        self.buffered_bytes() > 0
    }

    /// Returns `true` when the decoder holds a lone `ESC` byte that
    /// [`commit_escape()`](Self::commit_escape) would turn into the Escape key.
    ///
    /// A lone `ESC` is ambiguous: the terminal sends the same byte whether the
    /// user pressed the Escape key or started a sequence such as `ESC [ A`.
    /// Waiting for more input reports Escape only once the next key arrives, and
    /// the two bytes are then read as one Alt+key sequence. An application that
    /// wants Escape promptly should therefore wait a short time while this
    /// returns `true`, then commit the byte with
    /// [`commit_escape()`](Self::commit_escape). Around 50 ms, the default of
    /// Vim's `ttimeoutlen`, is the usual choice. A longer wait risks gluing a
    /// following key onto the `ESC`; a shorter one risks mistaking a slow
    /// sequence for the Escape key.
    pub fn has_uncommitted_escape(&self) -> bool {
        // Only a lone `ESC` is decidable by a timeout. The other states that
        // hold bytes back (`ESC [`, `ESC O`, a partial CSI key, an unterminated
        // mouse report, a UTF-8 lead byte) are prefixes that need more bytes, so
        // a timeout could only discard them.
        self.buf.as_slice() == [0x1b].as_slice()
    }

    /// Commits a lone `ESC` byte held by the decoder as the Escape key.
    ///
    /// This is the second half of the wait described by
    /// [`has_uncommitted_escape()`](Self::has_uncommitted_escape): call it once
    /// the wait has elapsed. The committed Escape key is then returned by
    /// [`next()`](Self::next). A call is a no-op when the decoder holds no lone
    /// `ESC` byte, so it is harmless when `next()` already consumed the byte or
    /// it turned out to be the start of a sequence.
    pub fn commit_escape(&mut self) {
        if !self.has_uncommitted_escape() {
            return;
        }
        // Mark the lone `ESC` as the Escape key so `next()` emits it instead of
        // holding it back as an incomplete sequence.
        self.buf.clear();
        self.committed_escape = true;
    }
}

fn parse_input(bytes: &[u8]) -> (Option<Input>, usize) {
    if bytes.is_empty() {
        return (None, 0);
    }

    match bytes[0] {
        // Regular ASCII character (not escape or backspace)
        b if b < 0x80 && b != 0x1b && b != 0x7f => parse_ascii_char(bytes),
        // Escape key or escape sequence
        0x1b => parse_escape_sequence(bytes),
        // Backspace
        0x7f => (Some(create_key_input(false, false, KeyCode::Backspace)), 1),
        // UTF-8 characters
        b if b >= 0x80 => parse_utf8_char(bytes),
        // Unknown byte
        _ => (None, 1),
    }
}

fn parse_ascii_char(bytes: &[u8]) -> (Option<Input>, usize) {
    let byte = bytes[0];

    // Control characters (Ctrl+A through Ctrl+Z)
    if byte < 0x20 {
        let (ctrl, code) = match byte {
            0x0D => (false, KeyCode::Enter), // Enter
            0x09 => (false, KeyCode::Tab),   // Tab
            c => (true, KeyCode::Char((c + 0x60) as char)),
        };
        return (Some(create_key_input(ctrl, false, code)), 1);
    }

    // Regular ASCII characters
    (
        Some(create_key_input(false, false, KeyCode::Char(byte as char))),
        1,
    )
}

fn parse_escape_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    // Need at least 2 bytes for escape sequences
    if bytes.len() == 1 {
        return (None, 0);
    }

    match bytes[1] {
        b'[' => parse_csi_sequence(bytes),
        b'O' => parse_ss3_sequence(bytes),
        // Alt + character (ESC followed by a regular character)
        b if b < 0x80 && b != 0x1b && b != 0x5b && b != 0x4f => parse_alt_char(bytes),
        // Standalone ESC or unknown sequence
        _ => (Some(create_key_input(false, false, KeyCode::Escape)), 1),
    }
}

fn parse_alt_char(bytes: &[u8]) -> (Option<Input>, usize) {
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

    (Some(create_key_input(ctrl, true, code)), 2)
}

fn parse_csi_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    // Need at least 3 bytes for basic CSI sequences (ESC [ X)
    if bytes.len() < 3 {
        return (None, 0);
    }

    match bytes[2] {
        b'<' => parse_sgr_mouse_sequence(bytes),
        b'M' => parse_x10_mouse_sequence(bytes),
        b'A'..=b'D' | b'H' | b'F' | b'Z' => parse_simple_csi_key(bytes),
        b'1'..=b'6' => parse_complex_csi_key(bytes),
        _ => (None, 3), // Unknown CSI sequence
    }
}

fn parse_ss3_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    // Need at least 3 bytes for SS3 sequences (ESC O X)
    if bytes.len() < 3 {
        return (None, 0);
    }

    let code = match bytes[2] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        _ => return (None, 3), // Unknown SS3 sequence
    };

    (Some(create_key_input(false, false, code)), 3)
}

fn parse_simple_csi_key(bytes: &[u8]) -> (Option<Input>, usize) {
    let code = match bytes[2] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => KeyCode::BackTab,
        _ => return (None, 3),
    };

    (Some(create_key_input(false, false, code)), 3)
}

fn parse_complex_csi_key(bytes: &[u8]) -> (Option<Input>, usize) {
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
        (None, 0)
    } else {
        (None, 3)
    }
}

fn parse_modified_arrow_key(bytes: &[u8]) -> (Option<Input>, usize) {
    if !bytes[4].is_ascii_digit() {
        return (None, 6);
    }

    let modifier = bytes[4] - b'0';
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    let code = match bytes[5] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        _ => return (None, 6),
    };

    (Some(create_key_input(ctrl, alt, code)), 6)
}

fn parse_special_key_simple(bytes: &[u8]) -> (Option<Input>, usize) {
    let code = match bytes[2] {
        b'1' | b'7' => KeyCode::Home,
        b'2' => KeyCode::Insert,
        b'3' => KeyCode::Delete,
        b'4' | b'8' => KeyCode::End,
        b'5' => KeyCode::PageUp,
        b'6' => KeyCode::PageDown,
        _ => return (None, 4),
    };

    (Some(create_key_input(false, false, code)), 4)
}

fn parse_special_key_with_modifier(bytes: &[u8]) -> (Option<Input>, usize) {
    let code = match bytes[2] {
        b'1' | b'7' => KeyCode::Home,
        b'2' => KeyCode::Insert,
        b'3' => KeyCode::Delete,
        b'4' | b'8' => KeyCode::End,
        b'5' => KeyCode::PageUp,
        b'6' => KeyCode::PageDown,
        _ => return (None, 6),
    };

    if !bytes[4].is_ascii_digit() {
        return (None, 6);
    }

    let modifier = bytes[4] - b'0';
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    (Some(create_key_input(ctrl, alt, code)), 6)
}

fn parse_sgr_mouse_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    // Find the end of the sequence (M or m)
    let mut end_pos = None;
    for (i, &b) in bytes.iter().enumerate().skip(3) {
        if b == b'M' || b == b'm' {
            end_pos = Some(i);
            break;
        }
        if !(b.is_ascii_digit() || b == b';') {
            // The parameters are digits and semicolons, so this byte cannot be
            // part of a sequence no matter what arrives later. Drop the prefix
            // rather than waiting forever for a terminator that can never come.
            return (None, i);
        }
    }

    let end = match end_pos {
        Some(pos) => pos,
        None => return (None, 0), // Incomplete sequence
    };

    // Parse the parameters
    let params_str = match std::str::from_utf8(&bytes[3..end]) {
        Ok(s) => s,
        // Not a valid SGR sequence: drop it like any other unparseable input.
        Err(_) => return (None, end + 1),
    };

    let params: Vec<&str> = params_str.split(';').collect();
    if params.len() != 3 {
        return (None, end + 1); // Invalid parameter count
    }

    let (button, x, y) = match (
        params[0].parse::<u16>(),
        params[1].parse::<u16>(),
        params[2].parse::<u16>(),
    ) {
        (Ok(b), Ok(x), Ok(y)) => (b, x, y),
        _ => return (None, end + 1), // Invalid parameters
    };

    let mouse_input = create_sgr_mouse_input(button, x, y, bytes[end] == b'm');
    match mouse_input {
        Some(input) => (Some(Input::Mouse(input)), end + 1),
        None => (None, end + 1),
    }
}

fn parse_x10_mouse_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    if bytes.len() < 6 {
        return (None, 0);
    }

    let button_byte = bytes[3];
    let x = bytes[4] as u16;
    let y = bytes[5] as u16;

    let mouse_input = create_x10_mouse_input(button_byte, x, y);
    (Some(Input::Mouse(mouse_input)), 6)
}

fn parse_utf8_char(bytes: &[u8]) -> (Option<Input>, usize) {
    let width = match bytes[0] {
        b if b & 0xE0 == 0xC0 => 2,
        b if b & 0xF0 == 0xE0 => 3,
        b if b & 0xF8 == 0xF0 => 4,
        _ => 1,
    };

    if bytes.len() < width {
        return (None, 0); // Not enough bytes yet
    }

    match std::str::from_utf8(&bytes[0..width]) {
        Ok(s) if let Some(c) = s.chars().next() => (
            Some(create_key_input(false, false, KeyCode::Char(c))),
            width,
        ),
        _ => (None, 1), // Invalid UTF-8, discard first byte
    }
}

// Helper functions
fn create_key_input(ctrl: bool, alt: bool, code: KeyCode) -> Input {
    Input::Key(KeyInput { ctrl, alt, code })
}

fn create_sgr_mouse_input(button: u16, x: u16, y: u16, is_release: bool) -> Option<MouseInput> {
    let button_code = button & 0x03;
    let ctrl = (button & 0x10) != 0;
    let alt = (button & 0x08) != 0;
    let shift = (button & 0x04) != 0;
    let drag = (button & 0x20) != 0;

    let kind = if drag {
        MouseInputKind::Drag
    } else if is_release {
        match button_code {
            0 => MouseInputKind::LeftRelease,
            1 => MouseInputKind::MiddleRelease,
            2 => MouseInputKind::RightRelease,
            _ => return None,
        }
    } else {
        // Check for scroll events first
        match button {
            64 => MouseInputKind::ScrollUp,
            65 => MouseInputKind::ScrollDown,
            _ => match button_code {
                0 => MouseInputKind::LeftPress,
                1 => MouseInputKind::MiddlePress,
                2 => MouseInputKind::RightPress,
                _ => return None,
            },
        }
    };

    Some(MouseInput {
        kind,
        position: Position {
            row: y.saturating_sub(1) as usize,
            col: x.saturating_sub(1) as usize,
        },
        ctrl,
        alt,
        shift,
    })
}

fn create_x10_mouse_input(button_byte: u8, x: u16, y: u16) -> MouseInput {
    let ctrl = (button_byte & 0x10) != 0;
    let alt = (button_byte & 0x08) != 0;
    let shift = (button_byte & 0x04) != 0;

    let kind = match button_byte {
        96 => MouseInputKind::ScrollUp,
        97 => MouseInputKind::ScrollDown,
        _ => {
            // Remove modifier bits to get the base button code
            let base_button = button_byte & !0x1C; // Remove shift(4), alt(8), ctrl(16) bits

            match base_button {
                32 => MouseInputKind::LeftPress,   // 0x20
                33 => MouseInputKind::MiddlePress, // 0x21
                34 => MouseInputKind::RightPress,  // 0x22
                35 => MouseInputKind::LeftRelease, // 0x23
                64 => MouseInputKind::Drag,        // 0x40
                _ => {
                    // Fallback: check bottom 2 bits for button type
                    match button_byte & 0x03 {
                        0 => MouseInputKind::LeftPress,
                        1 => MouseInputKind::MiddlePress,
                        2 => MouseInputKind::RightPress,
                        3 => MouseInputKind::LeftRelease,
                        _ => MouseInputKind::LeftPress,
                    }
                }
            }
        }
    };

    MouseInput {
        kind,
        position: Position {
            row: y.saturating_sub(33) as usize,
            col: x.saturating_sub(33) as usize,
        },
        ctrl,
        alt,
        shift,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn test_parse_regular_ascii_characters() {
        // Test regular ASCII characters
        let result = parse_input(b"a");
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"Z");
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('Z'),
            }))
        );
        assert_eq!(result.1, 1);

        let result = parse_input(b"5");
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x01]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 1);

        // Test Ctrl+Z (0x1A)
        let result = parse_input(&[0x1A]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Char('z'),
            }))
        );
        assert_eq!(result.1, 1);

        // Test Enter (0x0D)
        let result = parse_input(&[0x0D]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Enter,
            }))
        );
        assert_eq!(result.1, 1);

        // Test Tab (0x09)
        let result = parse_input(&[0x09]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Tab,
            }))
        );
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_parse_backspace() {
        let result = parse_input(&[0x7F]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b]);
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // ESC followed by unknown character should be treated as ESC
        let result = parse_input(&[0x1b, b'x']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'a']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(result.1, 2);

        // Alt+Enter
        let result = parse_input(&[0x1b, 0x0D]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Enter,
            }))
        );
        assert_eq!(result.1, 2);

        // Alt+Tab
        let result = parse_input(&[0x1b, 0x09]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'[', b'A']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 3);

        // Down arrow: ESC [ B
        let result = parse_input(&[0x1b, b'[', b'B']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Down,
            }))
        );
        assert_eq!(result.1, 3);

        // Right arrow: ESC [ C
        let result = parse_input(&[0x1b, b'[', b'C']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Right,
            }))
        );
        assert_eq!(result.1, 3);

        // Left arrow: ESC [ D
        let result = parse_input(&[0x1b, b'[', b'D']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'O', b'A']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 3);

        // Down arrow: ESC O B
        let result = parse_input(&[0x1b, b'O', b'B']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'[', b'H']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Home,
            }))
        );
        assert_eq!(result.1, 3);

        // End: ESC [ F
        let result = parse_input(&[0x1b, b'[', b'F']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::End,
            }))
        );
        assert_eq!(result.1, 3);

        // Home: ESC O H
        let result = parse_input(&[0x1b, b'O', b'H']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Home,
            }))
        );
        assert_eq!(result.1, 3);

        // End: ESC O F
        let result = parse_input(&[0x1b, b'O', b'F']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'[', b'Z']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::BackTab,
            }))
        );
        assert_eq!(result.1, 3);

        // Insert: ESC [ 2 ~
        let result = parse_input(&[0x1b, b'[', b'2', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Insert,
            }))
        );
        assert_eq!(result.1, 4);

        // Delete: ESC [ 3 ~
        let result = parse_input(&[0x1b, b'[', b'3', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Delete,
            }))
        );
        assert_eq!(result.1, 4);

        // Page Up: ESC [ 5 ~
        let result = parse_input(&[0x1b, b'[', b'5', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::PageUp,
            }))
        );
        assert_eq!(result.1, 4);

        // Page Down: ESC [ 6 ~
        let result = parse_input(&[0x1b, b'[', b'6', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'5', b'A']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(result.1, 6);

        // Alt+Right: ESC [ 1 ; 3 C (modifier 3 = Alt)
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'C']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Right,
            }))
        );
        assert_eq!(result.1, 6);

        // Ctrl+Alt+Left: ESC [ 1 ; 7 D (modifier 7 = Ctrl+Alt)
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'7', b'D']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0x1b, b'[', b'3', b';', b'5', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: true,
                alt: false,
                code: KeyCode::Delete,
            }))
        );
        assert_eq!(result.1, 6);

        // Alt+Home: ESC [ 1 ; 3 ~
        let result = parse_input(&[0x1b, b'[', b'1', b';', b'3', b'~']);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
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
        let result = parse_input(&[0xC3, 0xA9]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('é'),
            }))
        );
        assert_eq!(result.1, 2);

        // Test 3-byte UTF-8 character (€ = 0xE2 0x82 0xAC)
        let result = parse_input(&[0xE2, 0x82, 0xAC]);
        assert_eq!(
            result.0,
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('€'),
            }))
        );
        assert_eq!(result.1, 3);

        // Test incomplete UTF-8 sequence
        let result = parse_input(&[0xC3]);
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_incomplete_sequences() {
        // Incomplete escape sequence
        let result = parse_input(&[0x1b, b'[']);
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete special key sequence
        let result = parse_input(&[0x1b, b'[', b'2']);
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);

        // Incomplete modified key sequence
        let result = parse_input(&[0x1b, b'[', b'1', b';']);
        assert_eq!(result.0, None); // Need more bytes
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_input(&[]);
        assert_eq!(result.0, None);
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_unknown_sequences() {
        // Unknown escape sequence should be discarded
        let result = parse_input(&[0x1b, b'[', b'X']);
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Unknown ESC O sequence
        let result = parse_input(&[0x1b, b'O', b'X']);
        assert_eq!(result.0, None);
        assert_eq!(result.1, 3);

        // Invalid UTF-8 sequence
        let result = parse_input(&[0xFF]);
        assert_eq!(result.0, None);
        assert_eq!(result.1, 1);
    }

    #[test]
    fn test_input_decoder_preserves_incomplete_sequence_across_pushes() {
        let mut buffer = InputDecoder::new();

        // An incomplete escape sequence stays in the buffer.
        buffer.feed(&[0x1b, b'[']);
        assert_eq!(buffer.next(), None);
        assert!(buffer.has_pending());

        // The continuation completes the sequence.
        buffer.feed(b"A");
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert!(!buffer.has_pending());
    }

    #[test]
    fn test_input_decoder_drains_partial_bytes_in_order() {
        let mut buffer = InputDecoder::new();

        // Feed multiple complete inputs plus a trailing incomplete byte.
        buffer.feed(b"ab\x1b[");
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('b'),
            }))
        );
        // The partial CSI sequence remains and is reported via has_pending().
        assert_eq!(buffer.next(), None);
        assert!(buffer.has_pending());
    }

    #[test]
    fn test_input_decoder_buffered_bytes_and_discard() {
        let mut input = InputDecoder::new();
        assert_eq!(input.buffered_bytes(), 0);

        // An SGR mouse prefix that is never terminated keeps growing until the
        // application decides to drop it.
        let mut prefix = b"\x1b[<".to_vec();
        prefix.extend(std::iter::repeat_n(b'1', 10_000));
        input.feed(&prefix);
        assert_eq!(input.buffered_bytes(), prefix.len());

        // `len` is clipped to the buffer, and the return value is the count of
        // bytes actually discarded.
        const MAX_BUFFERED_BYTES: usize = 4096;
        let excess = input.buffered_bytes().saturating_sub(MAX_BUFFERED_BYTES);
        assert_eq!(input.discard_buffered_bytes(excess), excess);
        assert_eq!(input.buffered_bytes(), MAX_BUFFERED_BYTES);

        // Passing more than the buffered length clears the buffer and reports the
        // number of bytes that were held.
        assert_eq!(input.discard_buffered_bytes(usize::MAX), MAX_BUFFERED_BYTES);
        assert_eq!(input.buffered_bytes(), 0);
        assert!(!input.has_pending());
    }

    #[test]
    fn test_input_decoder_recovers_from_unterminated_mouse_prefix() {
        let mut input = InputDecoder::new();

        // A byte that cannot occur in the parameters of an SGR sequence means the
        // prefix can never become a valid sequence. Only the prefix is dropped, so
        // the input that follows is still parsed.
        input.feed(b"\x1b[<12a");
        assert_eq!(
            input.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert!(!input.has_pending());

        // A sequence that is still incomplete keeps waiting for its terminator.
        input.feed(b"\x1b[<12");
        assert_eq!(input.next(), None);
        assert!(input.has_pending());
    }

    #[test]
    fn test_input_decoder_reports_only_a_lone_esc_as_pending_escape() {
        // A lone `ESC` is the only held state that a timeout can commit.
        let mut input = InputDecoder::new();
        input.feed(b"\x1b");
        assert!(input.has_uncommitted_escape());

        // `ESC ESC` is the Escape key followed by another lone `ESC`.
        input.feed(b"\x1b");
        assert_eq!(
            input.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Escape,
            }))
        );
        assert!(input.has_uncommitted_escape());

        // A sequence prefix and a UTF-8 lead byte need more bytes, not a
        // timeout.
        for prefix in [b"\x1b[".as_slice(), b"\x1bO".as_slice(), b"\xe3".as_slice()] {
            let mut input = InputDecoder::new();
            input.feed(prefix);
            assert!(
                !input.has_uncommitted_escape(),
                "unexpected pending escape for {prefix:?}"
            );
        }

        // `ESC` followed by a regular character is an Alt+key sequence.
        let mut input = InputDecoder::new();
        input.feed(b"\x1ba");
        assert!(!input.has_uncommitted_escape());
        assert_eq!(
            input.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: true,
                code: KeyCode::Char('a'),
            }))
        );
    }

    #[test]
    fn test_input_decoder_commit_escape() {
        let mut input = InputDecoder::new();

        // Nothing is held, so there is nothing to commit and nothing comes out.
        input.commit_escape();
        assert_eq!(input.next(), None);

        // A lone `ESC` is committed as the Escape key, which `next()` then
        // returns and consumes.
        input.feed(b"\x1b");
        input.commit_escape();
        assert_eq!(
            input.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Escape,
            }))
        );
        assert!(!input.has_pending());
        assert_eq!(input.next(), None);
        input.commit_escape();
        assert_eq!(input.next(), None);

        // A sequence prefix is left for the parser to complete.
        input.feed(b"\x1b[");
        input.commit_escape();
        input.feed(b"A");
        assert_eq!(
            input.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        input.commit_escape();
        assert_eq!(input.next(), None);
    }

    #[test]
    fn test_input_decoder() {
        let mut buffer = InputDecoder::new();

        // A simple character.
        buffer.feed(b"a");
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );

        // An arrow key.
        buffer.feed(&[0x1b, b'[', b'A'][..]);
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );

        // Multiple inputs in one push.
        buffer.feed(b"ab");
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char('a'),
            }))
        );
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::ScrollUp,
                position: Position { row: 4, col: 9 }, // row: 5-1, col: 10-1
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode scroll down: ESC [ < 65 ; 10 ; 5 M
        let input = b"\x1b[<65;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::ScrollDown,
                position: Position { row: 4, col: 9 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 }, // row: 5-1, col: 10-1
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(result.1, input.len());

        // SGR mode middle button press: ESC [ < 1 ; 10 ; 5 M
        let input = b"\x1b[<1;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::MiddlePress,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode right button press: ESC [ < 2 ; 10 ; 5 M
        let input = b"\x1b[<2;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::RightPress,
                position: Position { row: 4, col: 9 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftRelease,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode middle button release: ESC [ < 1 ; 10 ; 5 m
        let input = b"\x1b[<1;10;5m";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::MiddleRelease,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode right button release: ESC [ < 2 ; 10 ; 5 m
        let input = b"\x1b[<2;10;5m";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::RightRelease,
                position: Position { row: 4, col: 9 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
                ctrl: true,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode with Alt modifier: ESC [ < 8 ; 10 ; 5 M (8 = 0 + 8)
        let input = b"\x1b[<8;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: true,
                shift: false,
            }))
        );

        // SGR mode with Shift modifier: ESC [ < 4 ; 10 ; 5 M (4 = 0 + 4)
        let input = b"\x1b[<4;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: true,
            }))
        );

        // SGR mode with all modifiers: ESC [ < 28 ; 10 ; 5 M (28 = 0 + 4 + 8 + 16)
        let input = b"\x1b[<28;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::Drag,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // SGR mode drag with modifiers: ESC [ < 60 ; 10 ; 5 M (60 = 0 + 4 + 8 + 16 + 32)
        let input = b"\x1b[<60;10;5M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::Drag,
                position: Position { row: 4, col: 9 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 5, col: 10 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(result.1, 6);

        // X10/X11 mode middle button press: ESC [ M <button> <x> <y>
        // Button 33 (0x21) = middle press
        let input = b"\x1b[M!\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::MiddlePress,
                position: Position { row: 5, col: 10 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode right button press: ESC [ M <button> <x> <y>
        // Button 34 (0x22) = right press
        let input = b"\x1b[M\"\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::RightPress,
                position: Position { row: 5, col: 10 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode button release: ESC [ M <button> <x> <y>
        // Button 35 (0x23) = release
        let input = b"\x1b[M#\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftRelease,
                position: Position { row: 5, col: 10 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 5, col: 10 },
                ctrl: true,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode with Alt modifier: button = 32 + 8 = 40 (0x28)
        let input = b"\x1b[M(\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 5, col: 10 },
                ctrl: false,
                alt: true,
                shift: false,
            }))
        );

        // X10/X11 mode with Shift modifier: button = 32 + 4 = 36 (0x24)
        let input = b"\x1b[M$\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 5, col: 10 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::ScrollUp,
                position: Position { row: 5, col: 10 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 mode scroll down: button = 97 (0x61)
        let input = b"\x1b[Ma\x2b\x26";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::ScrollDown,
                position: Position { row: 5, col: 10 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::Drag,
                position: Position { row: 5, col: 10 },
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 0, col: 0 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // Test large coordinates
        let input = b"\x1b[<0;100;200M";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 199, col: 99 }, // row: 200-1, col: 100-1
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
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 0, col: 0 }, // saturating_sub(1) on 0 = 0
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // X10/X11 sequence with minimum coordinate values (33)
        let input = b"\x1b[M !!";
        let result = parse_input(input);
        assert_eq!(
            result.0,
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 0, col: 0 }, // 33-33 = 0
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
    }

    #[test]
    fn test_input_decoder_mouse_inputs() {
        let mut buffer = InputDecoder::new();

        // A mouse click.
        buffer.feed(b"\x1b[<0;10;5M");
        assert_eq!(
            buffer.next(),
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );

        // Multiple mouse events in one push.
        buffer.feed(b"\x1b[<0;10;5M\x1b[<0;10;5m");
        assert_eq!(
            buffer.next(),
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftPress,
                position: Position { row: 4, col: 9 },
                ctrl: false,
                alt: false,
                shift: false,
            }))
        );
        assert_eq!(
            buffer.next(),
            Some(Input::Mouse(MouseInput {
                kind: MouseInputKind::LeftRelease,
                position: Position { row: 4, col: 9 },
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
        // Half of the cases end with a lone `ESC`, the state that a timeout has
        // to resolve. Sampling it structurally keeps that path covered instead
        // of depending on a partial sequence happening to land at the very end.
        if noprop::sample_bool(ctx) {
            bytes.push(0x1b);
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
        match noprop::sample_weighted_index(ctx, &[3, 2, 4, 1]) {
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
            2 => {
                let ctrl = noprop::sample_bool(ctx);
                let alt = noprop::sample_bool(ctx);
                let code = if ctrl {
                    KeyCode::Char(noprop::sample_choice(ctx, CTRL_CHARS))
                } else if alt {
                    KeyCode::Char(noprop::sample_choice(ctx, ALT_CHARS))
                } else {
                    KeyCode::Char(
                        char::from_u32(noprop::sample_usize_in(ctx, 0x21..=0x7e) as u32)
                            .expect("valid ASCII"),
                    )
                };
                KeyInput { ctrl, alt, code }
            }
            _ => KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Char(noprop::sample_choice(ctx, MULTIBYTE_CHARS)),
            },
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
    /// Regression guard: feeding `ESC [ 1 ; ! A` (a non-digit byte at
    /// the modifier position) must not panic. Such a sequence returns
    /// `(None, 6)` as an unknown CSI sequence from
    /// `parse_modified_arrow_key` / `parse_special_key_with_modifier`.
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
                match noprop::sample_usize_in(ctx, 0..4) {
                    0 => vec![0x1b],
                    1 => vec![0x1b, b'['],
                    2 => vec![0x1b, b'O'],
                    _ => vec![0x1b, b'[', b'1', b';', b'!', b'A'],
                }
            } else {
                sample_pbt_bytes(ctx)
            };
            let (input, consumed) = parse_input(&bytes);
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
            let (input, consumed) = parse_input(&bytes);
            assert_eq!(
                input,
                Some(Input::Key(key)),
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

    /// `InputDecoder::next` must agree with a model that applies
    /// `parse_input` repeatedly to the same bytes: the same event
    /// sequence, stopping at the same incomplete or fully consumed
    /// sequence.
    #[test]
    fn pbt_input_decoder_matches_parse_model() -> noprop::TestResult {
        let observed_input = Cell::new(false);
        let observed_partial = Cell::new(false);
        let observed_unknown = Cell::new(false);
        let observed_pending_escape = Cell::new(false);
        let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
        let mut runner = noprop::Runner::new(seed);
        runner.run(256, |ctx| {
            let bytes = sample_pbt_fragments(ctx);
            let mut buffer = InputDecoder::new();
            buffer.feed(&bytes);
            let mut actual = Vec::new();
            let actual_partial;
            loop {
                match buffer.next() {
                    Some(input) => actual.push(input),
                    None => {
                        actual_partial = buffer.has_pending();
                        break;
                    }
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
                let (input, consumed) = parse_input(rest);
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
            // A lone `ESC` is the only held state a timeout can resolve, and its
            // model is the bytes the parse could not consume.
            let pending_escape = rest == b"\x1b".as_slice();
            assert_eq!(
                buffer.has_uncommitted_escape(),
                pending_escape,
                "pending-escape mismatch for {bytes:?}"
            );
            if pending_escape {
                observed_pending_escape.set(true);
                buffer.commit_escape();
                assert_eq!(
                    buffer.next(),
                    Some(Input::Key(KeyInput {
                        ctrl: false,
                        alt: false,
                        code: KeyCode::Escape,
                    })),
                    "commit_escape must yield the Escape key for {bytes:?}"
                );
                assert!(
                    !buffer.has_pending(),
                    "commit_escape must consume the ESC for {bytes:?}"
                );
            } else {
                buffer.commit_escape();
                assert_eq!(
                    buffer.next(),
                    None,
                    "nothing should be committed for {bytes:?}"
                );
            }
            if !actual.is_empty() {
                observed_input.set(true);
            }
            if expected_partial {
                observed_partial.set(true);
            }
            if expected_unknown {
                observed_unknown.set(true);
            }
            Ok(())
        })?;
        assert!(observed_input.get(), "no case parsed any input\n{runner}");
        assert!(
            observed_partial.get(),
            "no case stopped at an incomplete sequence\n{runner}"
        );
        assert!(
            observed_unknown.get(),
            "no case consumed an unknown sequence\n{runner}"
        );
        assert!(
            observed_pending_escape.get(),
            "no case ended holding a lone ESC\n{runner}"
        );
        Ok(())
    }
}
