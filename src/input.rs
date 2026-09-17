use crate::Position;

/// User input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Input {
    /// A key event.
    Key(KeyInput),

    /// A mouse event: a button press or release, a drag, or a wheel scroll.
    Mouse(MouseInput),

    /// Bytes the decoder consumed but could not turn into an event.
    ///
    /// The decoder gives up on a sequence it cannot decode rather than
    /// holding it back, so these bytes are gone from the buffer; they are
    /// reported here so a mis-decode does not look like silence. The payload
    /// is the raw sequence, because that is what a caller can log and what
    /// cannot be recovered once it is dropped. It is also the way back from a
    /// decode the caller disagrees with: the bytes are handed over unmodified,
    /// and they are exactly what arrived, introducer and all. A caller that
    /// would have read those bytes differently can do so itself, without
    /// having to ask the decoder for a second opinion.
    ///
    /// That matters where a byte sequence has more than one reading and only
    /// the decoder gets to pick. `ESC ]` is one: the standard assigns it to a
    /// control string, so an application that wanted `Alt+]` finds those bytes
    /// here instead. `ESC P` and `ESC _` are the same shape.
    ///
    /// Nothing bounds the length of the payload, so cap what you keep; see
    /// the crate documentation for the shape of such a loop. This variant is
    /// also the only way the decoder reports giving up on bytes: the decoder
    /// never discards input it has not decoded, so nothing it holds is ever
    /// silently lost.
    Unrecognized {
        /// The bytes of the sequence that could not be decoded.
        bytes: Vec<u8>,
    },
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

    /// A function key, `F(1)` through `F(12)`.
    ///
    /// Values outside `1..=12` have no producer in the parser; they exist only
    /// because the range is not enforced by the type.
    F(u8),

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
    /// The left button went down.
    LeftPress,

    /// The left button came up.
    LeftRelease,

    /// The right button went down.
    RightPress,

    /// The right button came up.
    RightRelease,

    /// The middle button went down.
    MiddlePress,

    /// The middle button came up.
    MiddleRelease,

    /// The mouse moved while a button was held down.
    Drag,

    /// The wheel turned away from the user.
    ScrollUp,

    /// The wheel turned toward the user.
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
/// It does not bound how many bytes it holds, and it does not offer a way to
/// drop them: holding bytes is how it waits for the rest of a sequence, so
/// discarding them would mean decoding a stream whose sequence boundaries are
/// already lost. An application that reads from a source it does not control
/// should watch [`InputDecoder::buffered_bytes()`](Self::buffered_bytes) and
/// treat a buffer that keeps growing as a broken source rather than something to
/// recover from.
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
    /// [`buffered_bytes()`](Self::buffered_bytes).
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
    ///
    /// A sequence the decoder cannot decode is not held back: its bytes are
    /// consumed and returned as [`Input::Unrecognized`]. So `None` never means
    /// "bytes were dropped"; a caller that wants to notice a mis-decode gets it
    /// as a value.
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

        let (input, consumed) = parse_input(&self.buf);
        if consumed > 0 {
            self.buf.drain(..consumed);
        }
        input
    }

    /// Returns the number of bytes buffered but not yet consumed by
    /// [`next()`](Self::next).
    ///
    /// The count includes an incomplete sequence that is being held for more
    /// bytes. It does not include the lone `ESC` byte that
    /// [`commit_escape()`](Self::commit_escape) has committed, which is held as a
    /// flag rather than as a byte and is reported by
    /// [`has_uncommitted_escape()`](Self::has_uncommitted_escape) until
    /// [`next()`](Self::next) yields it.
    ///
    /// Waiting for more bytes is the only reason the decoder holds any, so a
    /// count that keeps growing is a statement about the input rather than about
    /// the decoder: no well-formed sequence stays buffered forever, so a buffer
    /// that does not drain means the source is not speaking terminal input. That
    /// makes this count the whole of what a caller needs to enforce and act on a
    /// bound of its own; see the crate documentation for the shape of such a
    /// loop.
    pub fn buffered_bytes(&self) -> usize {
        self.buf.len()
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
    /// This reports the lone `ESC` byte while it is still pending; after
    /// [`commit_escape()`](Self::commit_escape) it returns `false` even though
    /// the committed Escape key has not been yielded yet. The committed byte is
    /// held outside the buffer, so [`buffered_bytes()`](Self::buffered_bytes)
    /// does not count it either.
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

/// A control sequence being decoded, together with where its parameters start.
///
/// The two-byte `ESC X` form and the one-byte C1 form are the same sequence
/// written two ways. The parsers for CSI, OSC, DCS, and APC only care about the
/// parameters, so they are written against this type and never have to know
/// which form arrived. Holding the whole slice rather than just the parameters
/// is what lets a settled sequence be reported as [`Input::Unrecognized`] with
/// the bytes exactly as they arrived.
struct ControlSequence<'a> {
    /// The input as it arrived, introducer and all.
    bytes: &'a [u8],
    /// The index of the first parameter byte.
    start: usize,
}

impl<'a> ControlSequence<'a> {
    /// The sequence as it arrived. This is what [`Input::Unrecognized`] reports.
    fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The parameters and everything after them, without the introducer.
    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.start..]
    }

    /// The parameter byte at `offset`, or `None` past the end of the input.
    fn get(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.start + offset).copied()
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
        // C1 control, which is an ESC-prefixed sequence written as one byte
        b if (0x80..0xa0).contains(&b) => parse_c1_sequence(bytes),
        // UTF-8 characters
        b if b >= 0x80 => parse_utf8_char(bytes),
        // Unknown byte
        _ => unrecognized(bytes, 1),
    }
}

/// Build the result for a sequence the decoder consumed but could not decode.
///
/// `len` is how many bytes of `bytes` the caller is settling on; those bytes
/// become the payload and are reported as consumed, so the same bytes are not
/// examined again. The payload is a copy because the caller is about to drain
/// them from the buffer.
fn unrecognized(bytes: &[u8], len: usize) -> (Option<Input>, usize) {
    (some_unrecognized(bytes, len), len)
}

/// Build a settled-but-undecodable [`Input`] from the first `len` bytes.
///
/// This is the value half of [`unrecognized()`], for callers that have already
/// decided how many bytes to consume.
fn some_unrecognized(bytes: &[u8], len: usize) -> Option<Input> {
    Some(Input::Unrecognized {
        bytes: bytes[..len].to_vec(),
    })
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
        b'[' => parse_sequence(&ControlSequence { bytes, start: 2 }),
        b'O' => parse_ss3_sequence(bytes),
        // OSC, DCS, and APC: a control string with an opaque body. These are
        // matched before the Alt branch below, which would otherwise read the
        // introducer as Alt+character and spill the body out as keystrokes.
        //
        // This makes `ESC ]` an OSC introducer rather than Alt+`]`. The two are
        // the same bytes and are told apart only by what follows, so the choice
        // is forced; OSC is what the standard assigns to the sequence. A caller
        // that wants Alt+`]` can still read the report back as such, the same
        // way as for any other sequence reported in [`Input::Unrecognized`].
        // `ESC P` and `ESC _` are settled the same way.
        b']' | b'P' | b'_' => {
            parse_control_string(&ControlSequence { bytes, start: 2 }, bytes[1] == b']')
        }
        // Alt + character (ESC followed by a regular character)
        b if b < 0x80 && b != 0x1b && b != 0x5b && b != 0x4f => parse_alt_char(bytes),
        // Standalone ESC or unknown sequence
        _ => (Some(create_key_input(false, false, KeyCode::Escape)), 1),
    }
}

/// Decode a CSI sequence whose introducer has already been identified.
///
/// This is the `ESC [` / `0x9b` family. The introducer is not part of the
/// decision below: the two forms differ only in where the parameters start,
/// which [`ControlSequence`] already records.
fn parse_sequence(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    match seq.get(0) {
        Some(b'<') => parse_sgr_mouse_sequence(seq),
        Some(b'M') => parse_x10_mouse_sequence(seq),
        Some(b'A'..=b'D' | b'H' | b'F' | b'Z') => parse_simple_csi_key(seq),
        Some(b'1'..=b'6') => parse_complex_csi_key(seq),
        // A CSI sequence with no parameter byte of its own is incomplete, not a
        // settled sequence: `ESC [` and `0x9b` both wait for what follows.
        None => (None, 0),
        // Any other parameter byte ends a CSI sequence whose length is not
        // fixed: the terminator is whatever byte ends the parameter run, and
        // reporting a constant here would leave the tail in the buffer to spill
        // out as characters. A parameter run that reaches the end of the buffer
        // might still grow into a sequence a branch above claims, so only a
        // settled run is consumed.
        Some(_) => match find_parameter_terminator(seq) {
            Some(terminator) => unrecognized(seq.bytes(), terminator + 1),
            None => (None, 0),
        },
    }
}

/// Decode a sequence whose introducer is a single C1 byte.
///
/// The C1 range (`0x80..=0x9f`) is the ESC-prefixed introducers written as one
/// byte: `0x9b` is `CSI`, `0x9d` is `OSC`, `0x90` is `DCS`, and `0x9f` is `APC`.
/// A terminal that speaks 8-bit controls sends these instead of the equivalent
/// `ESC [`, `ESC ]`, `ESC P`, and `ESC _`, so an application that only knows the
/// two-byte forms would read `0x9b A` as an undecodable byte followed by `A`
/// rather than as Cursor Up.
///
/// Only the four introducers above are translated. The rest of the range is
/// settled as a single byte of [`Input::Unrecognized`], which is what the
/// generic undecodable path would do with it anyway.
///
/// A consequence is that `0x9b` is a CSI introducer and not `Alt+[`. The two
/// are the same byte with nothing to tell them apart, so the choice is forced;
/// a caller that wanted `Alt+[` can read the report back as such, the same way
/// as for `ESC ]` above.
fn parse_c1_sequence(bytes: &[u8]) -> (Option<Input>, usize) {
    let seq = ControlSequence { bytes, start: 1 };
    match bytes[0] {
        0x9b => parse_sequence(&seq),
        0x9d => parse_control_string(&seq, true),
        0x90 | 0x9f => parse_control_string(&seq, false),
        _ => unrecognized(bytes, 1),
    }
}

/// Decode an OSC, DCS, or APC control string.
///
/// The body is not interpreted; it is settled whole as [`Input::Unrecognized`],
/// which keeps it from reaching the application as the keys it was never typed
/// as. Reporting the introducer and waiting would be worse than reporting too
/// much: `ESC _ G ... ESC \\` from the kitty graphics protocol is one image, and
/// the application sees it either as one value or as tens of thousands of key
/// events.
///
/// `allow_bel` says the introducer was OSC, the only one of the three whose
/// body may end at `BEL` instead of `ST`. Either spelling of `ST` (`ESC \\` or
/// the single byte `0x9c`) ends a body.
fn parse_control_string(seq: &ControlSequence<'_>, allow_bel: bool) -> (Option<Input>, usize) {
    match find_control_string_end(seq, allow_bel) {
        Some(len) => unrecognized(seq.bytes(), len),
        None => (None, 0),
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
        // Unknown SS3 sequence. SS3 has no parameter run after `bytes[2]`, so
        // three bytes is the real length here.
        _ => return unrecognized(bytes, 3),
    };

    (Some(create_key_input(false, false, code)), 3)
}

fn parse_simple_csi_key(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let len = seq.start + 1;
    let code = match seq.get(0) {
        Some(b'A') => KeyCode::Up,
        Some(b'B') => KeyCode::Down,
        Some(b'C') => KeyCode::Right,
        Some(b'D') => KeyCode::Left,
        Some(b'H') => KeyCode::Home,
        Some(b'F') => KeyCode::End,
        Some(b'Z') => KeyCode::BackTab,
        _ => return unrecognized(seq.bytes(), len),
    };

    (Some(create_key_input(false, false, code)), len)
}

fn parse_complex_csi_key(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    // Handle sequences like ESC [ 1 A (an arrow with an explicit row count).
    //
    // ANSI defines `CSI A` as `CSI 1 A`, so a digit before the final byte is a
    // valid row count, not an unknown prefix. The count is discarded: `KeyCode`
    // cannot represent how far to move, and this keeps `ESC [ <digit> A` the
    // same shape as `ESC [ A` and `ESC [ 1 ; 5 A` (the modifier is what a key
    // event can carry; the count is not). Consuming the whole sequence here is
    // what stops a complete input from being held forever as "incomplete".
    let params = seq.rest();

    if params.len() >= 2 && params[0].is_ascii_digit() && matches!(params[1], b'A'..=b'D') {
        return parse_numbered_arrow_key(seq);
    }

    // Handle sequences like ESC [ 1 ; 5 A (modified arrow keys)
    if params.len() >= 4
        && params[0] == b'1'
        && params[1] == b';'
        && matches!(params[3], b'A'..=b'D')
    {
        return parse_modified_arrow_key(seq);
    }

    // Handle sequences like ESC [ 3 ~ (Delete) or ESC [ 3 ; 5 ~ (Ctrl+Delete)
    if params.len() >= 2 && params[1] == b'~' {
        return parse_special_key_simple(seq);
    }

    if params.len() >= 4 && params[1] == b';' && params[3] == b'~' {
        return parse_special_key_with_modifier(seq);
    }

    // Handle ESC [ <num> ~ and ESC [ <num> ; <mod> ~ (function keys). The
    // parameter is multi-digit, so it must be decoded from the whole run of
    // bytes up to the terminator rather than from the first parameter byte.
    if let Some(end) = find_tilde_terminator(seq) {
        return parse_tilde_key(seq, end);
    }

    // The parameter run ended in a byte that no branch above claims. That byte
    // is the terminator of a complete sequence, so consume the whole thing:
    // leaving it in the buffer would hold a complete input forever and, once
    // more bytes arrived, spill its tail out as ordinary characters.
    // `ESC [ 2 J` (clear screen) and `ESC [ 1 P` are the sequences this
    // catches.
    //
    // Only a terminator is enough to settle the sequence. A parameter run that
    // reaches the end of the buffer might still grow a `~` or an `A-D`, so that
    // case stays incomplete.
    if let Some(terminator) = find_parameter_terminator(seq) {
        return unrecognized(seq.bytes(), terminator + 1);
    }

    // The parameter run is still going and the buffer ends mid-sequence.
    (None, 0)
}

/// Return the index of the byte that ends the parameter run of an
/// `ESC [ <params> <terminator>` sequence, if the run is already terminated.
///
/// Parameters are digits and semicolons. The first other byte ends the run; a
/// run that reaches the end of the buffer without such a byte is incomplete, so
/// this returns `None` and the caller waits for more input.
fn find_parameter_terminator(seq: &ControlSequence<'_>) -> Option<usize> {
    for (i, &b) in seq.rest().iter().enumerate() {
        if !(b.is_ascii_digit() || b == b';') {
            return Some(seq.start + i);
        }
    }
    None
}

/// Return how many bytes the control string occupies, introducer and all, if its
/// terminator has arrived.
///
/// The introducer is already recorded in `seq`: it is `ESC ]`, `ESC P`, or
/// `ESC _` in the two-byte form, or `0x9d`, `0x90`, or `0x9f` in the one-byte
/// form. A control string body is opaque: it may contain `ESC` and, for OSC,
/// almost any byte but `BEL`, so the only thing the decoder can look for is the
/// terminator.
///
/// - OSC (`ESC ]`) ends at `BEL` (0x07) or at `ST`.
/// - DCS (`ESC P`) and APC (`ESC _`) end at `ST`.
///
/// `ST` is written either as `ESC \\` or as the single byte `0x9c`, and a
/// terminal that opens a control string with a one-byte introducer is likely to
/// close it with the one-byte terminator. Both spellings are accepted for all
/// three sequences.
///
/// Whichever terminator comes first wins: xterm sends OSC terminated by `BEL`,
/// but `ST` is equally valid and is the only terminator the other two use.
/// `None` means the body is still arriving, so the caller waits for more input.
fn find_control_string_end(seq: &ControlSequence<'_>, allow_bel: bool) -> Option<usize> {
    let bytes = seq.bytes();
    let mut i = seq.start;
    while i < bytes.len() {
        match bytes[i] {
            0x07 if allow_bel => return Some(i + 1),
            // `ST` as one byte. It is a terminator of the open body, not a byte
            // of the body, so it does not need to be quoted even though the
            // body is otherwise opaque.
            0x9c => return Some(i + 1),
            0x1b if bytes.get(i + 1) == Some(&b'\\') => return Some(i + 2),
            _ => i += 1,
        }
    }
    None
}

/// Return the index of the terminating `~` of an `ESC [ ... ~` sequence, if one
/// is present. The parameters are digits and semicolons; any other byte means
/// this is not a `~`-terminated sequence, so give up rather than wait for a
/// terminator that can never arrive.
fn find_tilde_terminator(seq: &ControlSequence<'_>) -> Option<usize> {
    for (i, &b) in seq.rest().iter().enumerate() {
        match b {
            b'~' => return Some(seq.start + i),
            b if b.is_ascii_digit() || b == b';' => continue,
            _ => return None,
        }
    }
    None
}

/// Decode an `ESC [ ... ~` sequence whose parameter text runs from `bytes[2]` up
/// to (but not including) the `~` at `end`. The parameter is either a single
/// key number, or a key number and a modifier separated by `;`.
///
/// A key number that maps to no key, or a parameter run that is not a pair of
/// numbers, is reported as [`Input::Unrecognized`]: the sequence is complete,
/// so it must not be left to leak its tail as ordinary characters.
fn parse_tilde_key(seq: &ControlSequence<'_>, end: usize) -> (Option<Input>, usize) {
    let bytes = seq.bytes();
    let len = end + 1;
    let params = match std::str::from_utf8(&bytes[seq.start..end]) {
        Ok(s) => s,
        Err(_) => return unrecognized(bytes, len),
    };

    let (number, modifier) = match params.split_once(';') {
        Some((number, modifier)) => (number, Some(modifier)),
        None => (params, None),
    };

    let number = match number.parse::<u8>() {
        Ok(n) => n,
        Err(_) => return unrecognized(bytes, len),
    };

    let (ctrl, alt) = match modifier {
        Some(modifier) => match modifier.parse::<u8>() {
            Ok(modifier) => (modifier & 0x4 != 0, modifier & 0x2 != 0),
            Err(_) => return unrecognized(bytes, len),
        },
        None => (false, false),
    };

    let code = match function_key_code(number) {
        Some(code) => code,
        None => return unrecognized(bytes, len),
    };

    (Some(create_key_input(ctrl, alt, code)), len)
}

/// Map an xterm-style `~` key number to the key it stands for.
///
/// The numbering is not an arithmetic sequence: `13` (F3 on some terminals, an
/// extra Enter on others), `16`, and `22` do not map to any key here, so this is
/// a table and not a formula.
fn function_key_code(number: u8) -> Option<KeyCode> {
    let code = match number {
        11 => KeyCode::F(1),
        12 => KeyCode::F(2),
        14 => KeyCode::F(4),
        15 => KeyCode::F(5),
        17 => KeyCode::F(6),
        18 => KeyCode::F(7),
        19 => KeyCode::F(8),
        20 => KeyCode::F(9),
        21 => KeyCode::F(10),
        23 => KeyCode::F(11),
        24 => KeyCode::F(12),
        _ => return None,
    };
    Some(code)
}

fn parse_numbered_arrow_key(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let len = seq.start + 2;
    let code = match seq.get(1) {
        Some(b'A') => KeyCode::Up,
        Some(b'B') => KeyCode::Down,
        Some(b'C') => KeyCode::Right,
        Some(b'D') => KeyCode::Left,
        // Unreachable: the caller only routes here when the parameter byte is
        // one of the four arrow bytes. Report the sequence instead of dropping
        // it, so a future change to the dispatch cannot silently leak the tail.
        _ => return unrecognized(seq.bytes(), len),
    };

    (Some(create_key_input(false, false, code)), len)
}

fn parse_modified_arrow_key(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let len = seq.start + 4;
    let modifier = match seq.get(2) {
        Some(b) if b.is_ascii_digit() => b - b'0',
        // The whole sequence is present, so it is settled: it just does not
        // name a key.
        _ => return unrecognized(seq.bytes(), len),
    };
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    let code = match seq.get(3) {
        Some(b'A') => KeyCode::Up,
        Some(b'B') => KeyCode::Down,
        Some(b'C') => KeyCode::Right,
        Some(b'D') => KeyCode::Left,
        // Unreachable, as in `parse_numbered_arrow_key`.
        _ => return unrecognized(seq.bytes(), len),
    };

    (Some(create_key_input(ctrl, alt, code)), len)
}

fn parse_special_key_simple(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let len = seq.start + 2;
    let code = match seq.get(0) {
        Some(b'1' | b'7') => KeyCode::Home,
        Some(b'2') => KeyCode::Insert,
        Some(b'3') => KeyCode::Delete,
        Some(b'4' | b'8') => KeyCode::End,
        Some(b'5') => KeyCode::PageUp,
        Some(b'6') => KeyCode::PageDown,
        // A digit that names no key, or a byte outside `1..=6` that the
        // dispatch let through. Either way the sequence is only these bytes.
        _ => return unrecognized(seq.bytes(), len),
    };

    (Some(create_key_input(false, false, code)), len)
}

fn parse_special_key_with_modifier(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let len = seq.start + 4;
    let code = match seq.get(0) {
        Some(b'1' | b'7') => KeyCode::Home,
        Some(b'2') => KeyCode::Insert,
        Some(b'3') => KeyCode::Delete,
        Some(b'4' | b'8') => KeyCode::End,
        Some(b'5') => KeyCode::PageUp,
        Some(b'6') => KeyCode::PageDown,
        _ => return unrecognized(seq.bytes(), len),
    };

    let modifier = match seq.get(2) {
        Some(b) if b.is_ascii_digit() => b - b'0',
        // The sequence is settled; the modifier is simply not one this parser
        // understands.
        _ => return unrecognized(seq.bytes(), len),
    };
    let alt = modifier & 0x2 != 0;
    let ctrl = modifier & 0x4 != 0;

    (Some(create_key_input(ctrl, alt, code)), len)
}

fn parse_sgr_mouse_sequence(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    let bytes = seq.bytes();
    // The parameters follow the marker byte that selected this parser.
    let params_start = seq.start + 1;

    // Find the end of the sequence (M or m)
    let mut end_pos = None;
    for (i, &b) in bytes.iter().enumerate().skip(params_start) {
        if b == b'M' || b == b'm' {
            end_pos = Some(i);
            break;
        }
        if !(b.is_ascii_digit() || b == b';') {
            // The parameters are digits and semicolons, so this byte cannot be
            // part of a sequence no matter what arrives later. The marker is
            // certainly part of no input, so settle it rather than waiting
            // forever for a terminator that can never come.
            //
            // Only the marker is consumed. The bytes after it are left in the
            // buffer: the parameters read so far are ordinary characters if
            // this was never an SGR report at all, and the caller should see
            // them as such.
            return (some_unrecognized(bytes, params_start), params_start);
        }
    }

    let end = match end_pos {
        Some(pos) => pos,
        None => return (None, 0), // Incomplete sequence
    };

    // Parse the parameters
    let params_str = match std::str::from_utf8(&bytes[params_start..end]) {
        Ok(s) => s,
        // The terminator settled the sequence, so it is reported as a whole
        // even though its parameters cannot be read.
        Err(_) => return unrecognized(bytes, end + 1),
    };

    let params: Vec<&str> = params_str.split(';').collect();
    if params.len() != 3 {
        return unrecognized(bytes, end + 1); // Invalid parameter count
    }

    let (button, x, y) = match (
        params[0].parse::<u16>(),
        params[1].parse::<u16>(),
        params[2].parse::<u16>(),
    ) {
        (Ok(b), Ok(x), Ok(y)) => (b, x, y),
        _ => return unrecognized(bytes, end + 1), // Invalid parameters
    };

    let mouse_input = create_sgr_mouse_input(button, x, y, bytes[end] == b'm');
    match mouse_input {
        Some(input) => (Some(Input::Mouse(input)), end + 1),
        None => unrecognized(bytes, end + 1),
    }
}

fn parse_x10_mouse_sequence(seq: &ControlSequence<'_>) -> (Option<Input>, usize) {
    // `M` plus the three payload bytes behind it.
    let len = seq.start + 4;
    if seq.bytes().len() < len {
        return (None, 0);
    }

    let button_byte = seq.get(1).expect("checked by the length test above");
    let x = seq.get(2).expect("checked by the length test above") as u16;
    let y = seq.get(3).expect("checked by the length test above") as u16;

    let mouse_input = create_x10_mouse_input(button_byte, x, y);
    (Some(Input::Mouse(mouse_input)), len)
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
        // Not valid UTF-8, so the lead byte cannot start a character. Report
        // the byte so the caller can see what was dropped.
        _ => unrecognized(bytes, 1),
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
    fn test_parse_numbered_arrow_keys() {
        // ANSI makes `CSI A` and `CSI 1 A` the same input, and `CSI 5 A` is
        // "up 5 rows". The count cannot be represented, so it is discarded and
        // every digit-prefixed arrow maps to the arrow direction.
        for digit in b'1'..=b'6' {
            for (suffix, code) in [
                (b'A', KeyCode::Up),
                (b'B', KeyCode::Down),
                (b'C', KeyCode::Right),
                (b'D', KeyCode::Left),
            ] {
                let bytes = [0x1b, b'[', digit, suffix];
                let result = parse_input(&bytes);
                assert_eq!(
                    result.0,
                    Some(Input::Key(KeyInput {
                        ctrl: false,
                        alt: false,
                        code,
                    })),
                    "ESC [{}{}] should be a known arrow, not held or dropped",
                    char::from(digit),
                    char::from(suffix),
                );
                // The whole sequence is consumed, so nothing is left pending.
                assert_eq!(result.1, bytes.len());
            }
        }
    }

    #[test]
    fn test_parse_function_keys_bare() {
        // The xterm `~` numbering is not an arithmetic sequence, so the table is
        // checked explicitly rather than derived.
        for (number, n) in [
            ("11", 1),
            ("12", 2),
            ("14", 4),
            ("15", 5),
            ("17", 6),
            ("18", 7),
            ("19", 8),
            ("20", 9),
            ("21", 10),
            ("23", 11),
            ("24", 12),
        ] {
            let seq = format!("\x1b[{number}~");
            let result = parse_input(seq.as_bytes());
            assert_eq!(
                result.0,
                Some(Input::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::F(n),
                })),
                "ESC [{number}~ should be F({n})"
            );
            assert_eq!(result.1, seq.len());
        }
    }

    #[test]
    fn test_parse_function_keys_with_modifier() {
        // The modified form shares the sequence shape and the modifier bits with
        // the modified arrows and `~` keys.
        // tuinix decodes the modifier byte with the same bits as the modified
        // arrows and `~` keys: bit 0x2 is Alt, bit 0x4 is Ctrl.
        let cases: &[(&[u8], bool, bool, u8)] = &[
            (b"\x1b[15;2~", false, true, 5),  // Shift+F5 (0x2)
            (b"\x1b[15;5~", true, false, 5),  // Ctrl+F5 (0x4)
            (b"\x1b[15;7~", true, true, 5),   // Ctrl+Alt+F5 (0x6)
            (b"\x1b[11;2~", false, true, 1),  // Shift+F1
            (b"\x1b[24;5~", true, false, 12), // Ctrl+F12
        ];

        for &(seq, ctrl, alt, n) in cases {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Key(KeyInput {
                    ctrl,
                    alt,
                    code: KeyCode::F(n),
                })),
                "{seq:?} should be F({n}) with ctrl={ctrl}, alt={alt}"
            );
            assert_eq!(result.1, seq.len());
        }
    }

    #[test]
    fn test_parse_unknown_tilde_sequences_consume_everything() {
        // A `~` sequence whose parameter tuinix does not map to a key is still a
        // complete sequence. It is reported as a whole; "~" and the extra digits
        // must not leak out as ordinary characters.
        for seq in [
            &b"\x1b[13~"[..], // F3 on some terminals, extra Enter on others
            &b"\x1b[16~"[..],
            &b"\x1b[22~"[..],
            &b"\x1b[25~"[..],
        ] {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                }),
                "{seq:?} should be reported whole"
            );
            assert_eq!(
                result.1,
                seq.len(),
                "{seq:?} should be consumed whole, not truncated"
            );
        }

        // A modifier makes the sequence longer but does not change the rule.
        for seq in [&b"\x1b[22;2~"[..], &b"\x1b[13;2~"[..]] {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                })
            );
            assert_eq!(result.1, seq.len());
        }
    }

    #[test]
    fn test_parse_unknown_terminator_sequences_consume_everything() {
        // A CSI sequence whose parameter is a digit but whose terminator is not
        // one tuinix maps to a key is still a complete sequence. `ESC [ 2 J` is
        // "clear screen", not "the letter J": the terminator must not leak out
        // as an ordinary character.
        for seq in [
            &b"\x1b[2J"[..],
            &b"\x1b[1P"[..],
            &b"\x1b[2M"[..],
            &b"\x1b[3L"[..],
            &b"\x1b[1;5X"[..],
        ] {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                }),
                "{seq:?} should be reported whole"
            );
            assert_eq!(
                result.1,
                seq.len(),
                "{seq:?} should be consumed whole, not truncated"
            );
        }
    }

    #[test]
    fn test_parse_single_digit_csi_with_unknown_terminator() {
        // The dispatch on `bytes[2]` sends a digit-led CSI sequence to the
        // complex parser, but its length is decided by the terminator, not by
        // the digit. `ESC [ 9 ~` used to report 3 bytes while the buffer held
        // 4, so `~` escaped as an ordinary character.
        let seq = b"\x1b[9~";
        let result = parse_input(seq);
        assert_eq!(
            result.0,
            Some(Input::Unrecognized {
                bytes: seq.to_vec()
            })
        );
        assert_eq!(result.1, seq.len());

        // A one-byte terminator that is not a digit parameter is still whole.
        let seq = b"\x1b[9X";
        let result = parse_input(seq);
        assert_eq!(
            result.0,
            Some(Input::Unrecognized {
                bytes: seq.to_vec()
            })
        );
        assert_eq!(result.1, seq.len());
    }

    #[test]
    fn test_parse_control_strings_consume_everything() {
        // OSC, DCS, and APC have opaque bodies. The body must not reach the
        // application as keystrokes, so the whole string is settled at once.
        for seq in [
            // OSC terminated by BEL (what xterm sends for a window title).
            &b"\x1b]0;hi\x07"[..],
            // OSC terminated by ST.
            &b"\x1b]0;hi\x1b\\"[..],
            // An OSC body may contain a lone ESC that is not a terminator.
            &b"\x1b]11;?\x1b[0m\x07"[..],
            // ... or an ESC that turns out not to be `ST`.
            &b"\x1b]11;?\x1bZ\x07"[..],
            // DCS terminated by ST.
            &b"\x1bP1$r0m\x1b\\"[..],
            // APC terminated by ST: the kitty graphics protocol.
            &b"\x1b_Ga=T;f=100;s=1;v=1\x1b\\"[..],
        ] {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                }),
                "{seq:?} should be reported whole"
            );
            assert_eq!(
                result.1,
                seq.len(),
                "{seq:?} should be consumed whole, not truncated"
            );
        }
    }

    #[test]
    fn test_parse_control_strings_ending_at_one_byte_st() {
        // `ST` is one byte (`0x9c`) as well as two (`ESC \\`). A terminal that
        // opens a control string with a one-byte introducer tends to close it
        // with the one-byte terminator, and that spelling has to end the body
        // too: otherwise the sequence is a complete input that never settles.
        for seq in [
            // OSC terminated by the 8-bit ST.
            &b"\x9d0;hi\x9c"[..],
            // The same body can still be written with the two-byte terminator.
            &b"\x9d0;hi\x1b\\"[..],
            // DCS terminated by the 8-bit ST.
            &b"\x901$r0m\x9c"[..],
            // APC terminated by the 8-bit ST.
            &b"\x9fGa=T;f=100\x9c"[..],
        ] {
            let result = parse_input(seq);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                }),
                "{seq:?} should be reported whole"
            );
            assert_eq!(result.1, seq.len(), "{seq:?} should be consumed whole");
        }
    }

    #[test]
    fn test_parse_bel_ends_osc_but_not_dcs_or_apc() {
        // `BEL` is a terminator for OSC alone. The other two bodies are opaque
        // and may contain it, so they wait for `ST`.
        let result = parse_input(b"\x9d0;hi\x07");
        assert_eq!(result.1, 6, "OSC should end at BEL");

        for seq in [&b"\x90body\x07"[..], &b"\x9fbody\x07"[..]] {
            let result = parse_input(seq);
            assert_eq!(result.0, None, "{seq:?} is not ended by BEL");
            assert_eq!(result.1, 0, "{seq:?} must consume nothing");
        }
    }

    #[test]
    fn test_parse_unterminated_control_string_waits() {
        // Nothing may be settled while the body is still arriving: reporting the
        // introducer early would be wrong, and reporting a prefix as a body would
        // lose the rest.
        for seq in [
            &b"\x1b]"[..],
            &b"\x1b]0;hi"[..],
            &b"\x1b]0;hi\x1b"[..],
            &b"\x1bP1$r"[..],
            &b"\x1b_Ga=T"[..],
            &b"\x1b_Ga=T\x1b"[..],
        ] {
            let result = parse_input(seq);
            assert_eq!(result.0, None, "{seq:?} is incomplete");
            assert_eq!(result.1, 0, "{seq:?} must consume nothing");
        }
    }

    #[test]
    fn test_parse_alt_character_is_not_a_control_string() {
        // The introducer characters are otherwise reachable as Alt+key. Matching
        // them as control strings is only correct when a terminator follows; a
        // lone `ESC ]` starts an OSC and must wait rather than being read as
        // Alt+`]`.
        let result = parse_input(b"\x1b]");
        assert_eq!(result.0, None);
        assert_eq!(result.1, 0);

        // A character that cannot introduce a control string still reads as Alt.
        let result = parse_input(b"\x1bX");
        assert_eq!(
            result.0,
            Some(create_key_input(false, true, KeyCode::Char('X')))
        );
        assert_eq!(result.1, 2);
    }

    #[test]
    fn test_parse_c1_introducers_are_their_escape_sequence() {
        // A terminal that sends 8-bit controls writes the introducer as a single
        // byte. `0x9b A` is Cursor Up, the same input as `ESC [ A`.
        // The input is one byte shorter, so the value matches and each form
        // consumes exactly its own length.
        for (c1, esc) in [
            (&b"\x9bA"[..], &b"\x1b[A"[..]),
            (&b"\x9b1;5A"[..], &b"\x1b[1;5A"[..]),
            (&b"\x9b3~"[..], &b"\x1b[3~"[..]),
            (&b"\x9b11~"[..], &b"\x1b[11~"[..]),
            (&b"\x9b<0;5;5M"[..], &b"\x1b[<0;5;5M"[..]),
        ] {
            let c1_result = parse_input(c1);
            let esc_result = parse_input(esc);
            assert_eq!(
                c1_result.0, esc_result.0,
                "{c1:?} should decode the same as {esc:?}"
            );
            assert_eq!(c1_result.1, c1.len(), "{c1:?} should be consumed whole");
            assert_eq!(esc_result.1, esc.len(), "{esc:?} should be consumed whole");
        }

        // Control strings: `0x9d` is OSC, `0x90` is DCS, `0x9f` is APC. `ST`
        // has an 8-bit spelling (`0x9c`) in this form, but the body is opaque
        // and is not scanned for C1 bytes, so the terminator is still `ESC \\`.
        // The report carries the bytes that arrived, so the two forms settle
        // different payloads: only the length and the fact that the body is one
        // value are shared.
        for (c1, esc) in [
            (&b"\x9d0;hi\x07"[..], &b"\x1b]0;hi\x07"[..]),
            (&b"\x9d0;hi\x1b\\"[..], &b"\x1b]0;hi\x1b\\"[..]),
            (&b"\x900$r0m\x1b\\"[..], &b"\x1bP0$r0m\x1b\\"[..]),
            (&b"\x9fGa=T\x1b\\"[..], &b"\x1b_Ga=T\x1b\\"[..]),
        ] {
            let c1_result = parse_input(c1);
            let esc_result = parse_input(esc);
            assert_eq!(
                c1_result.0,
                Some(Input::Unrecognized { bytes: c1.to_vec() }),
                "{c1:?} should settle as one report of the bytes that arrived"
            );
            assert_eq!(
                esc_result.0,
                Some(Input::Unrecognized {
                    bytes: esc.to_vec()
                }),
                "{esc:?} should settle as one report of the bytes that arrived"
            );
            assert_eq!(c1_result.1, c1.len(), "{c1:?} should be consumed whole");
            assert_eq!(esc_result.1, esc.len(), "{esc:?} should be consumed whole");
        }
    }

    #[test]
    fn test_parse_lone_c1_byte_waits() {
        // The introducer alone is incomplete, not an event.
        for c1 in [0x9b, 0x9d, 0x90, 0x9f] {
            let result = parse_input(&[c1]);
            assert_eq!(result.0, None, "{c1:#x} alone is incomplete");
            assert_eq!(result.1, 0, "{c1:#x} alone must consume nothing");
        }
    }

    #[test]
    fn test_parse_untranslated_c1_bytes_are_settled_one_at_a_time() {
        // The rest of the C1 range is not an introducer, so it is settled as a
        // single byte and does not swallow what follows it. `0x9c` is `ST`,
        // which only ends a control string that is already open; on its own it
        // is just an undecodable byte.
        for c1 in [0x80, 0x9c, 0x9e, 0x9a] {
            let result = parse_input(&[c1, b'A']);
            assert_eq!(
                result.0,
                Some(Input::Unrecognized { bytes: vec![c1] }),
                "{c1:#x} should be reported alone"
            );
            assert_eq!(result.1, 1, "{c1:#x} should consume one byte");
        }
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_input(&[]);
        assert_eq!(result.0, None);
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_parse_unknown_sequences() {
        // Unknown escape sequence is reported rather than silently discarded
        let result = parse_input(&[0x1b, b'[', b'X']);
        assert_eq!(
            result.0,
            Some(Input::Unrecognized {
                bytes: vec![0x1b, b'[', b'X']
            })
        );
        assert_eq!(result.1, 3);

        // Unknown ESC O sequence
        let result = parse_input(&[0x1b, b'O', b'X']);
        assert_eq!(
            result.0,
            Some(Input::Unrecognized {
                bytes: vec![0x1b, b'O', b'X']
            })
        );
        assert_eq!(result.1, 3);

        // Invalid UTF-8 sequence
        let result = parse_input(&[0xFF]);
        assert_eq!(result.0, Some(Input::Unrecognized { bytes: vec![0xFF] }));
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
    fn test_input_decoder_consumes_numbered_arrow_without_leaving_bytes() {
        // D1: a complete numbered arrow must consume its bytes so the buffer
        // does not grow without bound.
        let mut buffer = InputDecoder::new();
        buffer.feed(b"\x1b[5A");
        assert_eq!(
            buffer.next(),
            Some(Input::Key(KeyInput {
                ctrl: false,
                alt: false,
                code: KeyCode::Up,
            }))
        );
        assert_eq!(buffer.buffered_bytes(), 0);
        assert!(!buffer.has_pending());
    }

    #[test]
    fn test_input_decoder_numbered_arrow_matches_split_feed() {
        // D2: feeding the sequence byte by byte yields the same input and the
        // same drained buffer as feeding it whole.
        let mut whole = InputDecoder::new();
        whole.feed(b"\x1b[3C");

        let mut split = InputDecoder::new();
        for byte in b"\x1b[3C" {
            split.feed(&[*byte]);
        }

        assert_eq!(split.next(), whole.next());
        assert_eq!(split.buffered_bytes(), whole.buffered_bytes());
    }

    #[test]
    fn test_input_decoder_consumes_unknown_tilde_without_leaking_bytes() {
        // D1: a complete but unmapped `~` sequence must drain its bytes so the
        // buffer does not grow without bound and the tail does not surface as
        // character input.
        let mut buffer = InputDecoder::new();
        buffer.feed(b"\x1b[22~");
        assert_eq!(
            buffer.next(),
            Some(Input::Unrecognized {
                bytes: b"\x1b[22~".to_vec()
            })
        );
        assert_eq!(buffer.buffered_bytes(), 0);
        assert!(!buffer.has_pending());
    }

    #[test]
    fn test_input_decoder_unknown_tilde_matches_split_feed() {
        // D2: feeding the sequence byte by byte yields the same drained buffer
        // as feeding it whole.
        let mut whole = InputDecoder::new();
        whole.feed(b"\x1b[22~");

        let mut split = InputDecoder::new();
        for byte in b"\x1b[22~" {
            split.feed(&[*byte]);
        }

        assert_eq!(split.next(), whole.next());
        assert_eq!(split.buffered_bytes(), whole.buffered_bytes());
    }

    #[test]
    fn test_input_decoder_consumes_unknown_terminator_without_leaving_bytes() {
        // D1: a complete CSI sequence with an unmapped terminator must drain its
        // bytes. `ESC [ 2 J` is a complete sequence, so holding it is not an
        // incomplete input: a decoder that keeps it grows without bound while
        // the terminal keeps clearing the screen.
        for seq in [&b"\x1b[2J"[..], &b"\x1b[1P"[..], &b"\x1b[1;5X"[..]] {
            let mut buffer = InputDecoder::new();
            buffer.feed(seq);
            assert_eq!(
                buffer.next(),
                Some(Input::Unrecognized {
                    bytes: seq.to_vec()
                }),
                "{seq:?}"
            );
            assert_eq!(buffer.buffered_bytes(), 0, "{seq:?}");
            assert!(!buffer.has_pending(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_unknown_terminator_matches_split_feed() {
        // D2: feeding the sequence byte by byte yields the same drained buffer
        // as feeding it whole.
        for seq in [&b"\x1b[2J"[..], &b"\x1b[1;5X"[..]] {
            let mut whole = InputDecoder::new();
            whole.feed(seq);

            let mut split = InputDecoder::new();
            for byte in seq {
                split.feed(&[*byte]);
            }

            assert_eq!(split.next(), whole.next(), "{seq:?}");
            assert_eq!(split.buffered_bytes(), whole.buffered_bytes(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_consumes_function_key_without_leaving_bytes() {
        // D1: a complete function key sequence must consume its (variable
        // length) bytes so the buffer does not grow without bound.
        for seq in [&b"\x1b[11~"[..], &b"\x1b[15~"[..], &b"\x1b[15;2~"[..]] {
            let mut buffer = InputDecoder::new();
            buffer.feed(seq);
            assert!(matches!(buffer.next(), Some(Input::Key(_))), "{seq:?}");
            assert_eq!(buffer.buffered_bytes(), 0, "{seq:?}");
            assert!(!buffer.has_pending(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_function_key_matches_split_feed() {
        // D2: feeding the sequence byte by byte yields the same input and the
        // same drained buffer as feeding it whole.
        for seq in [&b"\x1b[11~"[..], &b"\x1b[15~"[..], &b"\x1b[15;2~"[..]] {
            let mut whole = InputDecoder::new();
            whole.feed(seq);

            let mut split = InputDecoder::new();
            for byte in seq {
                split.feed(&[*byte]);
            }

            assert_eq!(split.next(), whole.next(), "{seq:?}");
            assert_eq!(split.buffered_bytes(), whole.buffered_bytes(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_consumes_control_string_without_leaking_bytes() {
        // D1: a control string must drain in one call, however long its body is.
        // The whole point of the fix is that a body of arbitrary length reaches
        // the caller as one value instead of one key event per byte.
        let mut seq = b"\x1b_".to_vec();
        seq.push(b'G');
        seq.extend(std::iter::repeat_n(b'A', 4096));
        seq.extend_from_slice(b"\x1b\\");

        let mut buffer = InputDecoder::new();
        buffer.feed(&seq);
        assert_eq!(
            buffer.next(),
            Some(Input::Unrecognized { bytes: seq.clone() })
        );
        assert_eq!(buffer.buffered_bytes(), 0);
        assert!(!buffer.has_pending());

        // The same holds for an OSC, whose terminator may be BEL.
        for osc in [&b"\x1b]0;hi\x07"[..], &b"\x1b]0;hi\x1b\\"[..]] {
            let mut buffer = InputDecoder::new();
            buffer.feed(osc);
            assert_eq!(
                buffer.next(),
                Some(Input::Unrecognized {
                    bytes: osc.to_vec()
                }),
                "{osc:?}"
            );
            assert_eq!(buffer.buffered_bytes(), 0, "{osc:?}");
        }
    }

    #[test]
    fn test_input_decoder_control_string_matches_split_feed() {
        // D2: feeding the sequence byte by byte yields the same input and the
        // same drained buffer as feeding it whole, for both terminator forms.
        for seq in [
            &b"\x1b]0;hi\x07"[..],
            &b"\x1b]0;hi\x1b\\"[..],
            &b"\x1bP1$r0m\x1b\\"[..],
            &b"\x1b_Ga=T;f=100\x1b\\"[..],
        ] {
            let mut whole = InputDecoder::new();
            whole.feed(seq);

            let mut split = InputDecoder::new();
            for byte in seq {
                split.feed(&[*byte]);
            }

            assert_eq!(split.next(), whole.next(), "{seq:?}");
            assert_eq!(split.buffered_bytes(), whole.buffered_bytes(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_consumes_c1_control_string_without_leaking_bytes() {
        // D1: a C1 control string closed with the one-byte `ST` is a complete
        // input. It has to drain in a single call; before this was fixed the
        // decoder held all of it waiting for an `ESC \\` that never came.
        let mut seq = vec![0x9f];
        seq.push(b'G');
        seq.extend(std::iter::repeat_n(b'A', 4096));
        seq.push(0x9c);

        let mut buffer = InputDecoder::new();
        buffer.feed(&seq);
        assert_eq!(
            buffer.next(),
            Some(Input::Unrecognized { bytes: seq.clone() })
        );
        assert_eq!(buffer.next(), None);
        assert_eq!(buffer.buffered_bytes(), 0);
        assert!(!buffer.has_pending());
    }

    #[test]
    fn test_input_decoder_one_byte_st_matches_split_feed() {
        // D2: feeding a `0x9c`-terminated control string byte by byte yields the
        // same input and the same drained buffer as feeding it whole.
        for seq in [
            &b"\x9d0;hi\x9c"[..],
            &b"\x901$r0m\x9c"[..],
            &b"\x9fGa=T;f=100\x9c"[..],
        ] {
            let mut whole = InputDecoder::new();
            whole.feed(seq);

            let mut split = InputDecoder::new();
            for byte in seq {
                split.feed(&[*byte]);
            }

            assert_eq!(split.next(), whole.next(), "{seq:?}");
            assert_eq!(split.buffered_bytes(), whole.buffered_bytes(), "{seq:?}");
        }
    }

    #[test]
    fn test_input_decoder_yields_a_character_after_a_control_string() {
        // The bytes after a terminator are ordinary input, so the decoder must
        // not over-consume: an OSC followed by a key press yields the string and
        // then the key.
        let mut buffer = InputDecoder::new();
        buffer.feed(b"\x1b]0;hi\x07x");

        assert_eq!(
            buffer.next(),
            Some(Input::Unrecognized {
                bytes: b"\x1b]0;hi\x07".to_vec()
            })
        );
        assert_eq!(
            buffer.next(),
            Some(create_key_input(false, false, KeyCode::Char('x')))
        );
        assert_eq!(buffer.buffered_bytes(), 0);
    }

    #[test]
    fn test_input_decoder_consumes_c1_sequence_without_leaving_bytes() {
        // D1: one 8-bit CSI yields one key event and leaves nothing behind, so
        // the byte after it is not read as part of the sequence.
        let mut buffer = InputDecoder::new();
        buffer.feed(b"\x9bA");
        assert_eq!(
            buffer.next(),
            Some(create_key_input(false, false, KeyCode::Up))
        );
        assert_eq!(buffer.next(), None);
        assert_eq!(buffer.buffered_bytes(), 0);

        // A long 8-bit APC body becomes one value, not a flood of characters.
        let mut body = vec![0x9f];
        body.extend_from_slice(&[b'G'; 4096]);
        body.extend_from_slice(b"\x1b\\");

        let mut buffer = InputDecoder::new();
        buffer.feed(&body);
        assert_eq!(
            buffer.next(),
            Some(Input::Unrecognized { bytes: body }),
            "the body should be reported whole"
        );
        assert_eq!(buffer.next(), None);
        assert_eq!(buffer.buffered_bytes(), 0);
    }

    #[test]
    fn test_input_decoder_c1_sequence_matches_split_feed() {
        // D2: feeding a C1 sequence byte by byte yields the same input and the
        // same drained buffer as feeding it whole. The rebuilt `ESC`-prefixed
        // form is one byte longer than the input, so a mistake in mapping the
        // consumed count back would show up here as a lost or doubled byte.
        for seq in [
            &b"\x9bA"[..],
            &b"\x9b1;5A"[..],
            &b"\x9b11~"[..],
            &b"\x9b<0;5;5M"[..],
            &b"\x9d0;hi\x07"[..],
            &b"\x9fGa=T;f=100\x1b\\"[..],
            &b"\x9d0;hi"[..],
        ] {
            let mut whole = InputDecoder::new();
            whole.feed(seq);

            let mut split = InputDecoder::new();
            for byte in seq {
                split.feed(&[*byte]);
            }

            assert_eq!(split.next(), whole.next(), "{seq:?}");
            assert_eq!(split.buffered_bytes(), whole.buffered_bytes(), "{seq:?}");
        }
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
    fn test_input_decoder_buffered_bytes_grow_without_draining() {
        let mut input = InputDecoder::new();
        assert_eq!(input.buffered_bytes(), 0);

        // A CSI parameter run that never reaches a terminator keeps growing:
        // holding bytes is how the decoder waits for the rest of the sequence,
        // so the count is all the application has to notice that the wait never
        // ends. Nothing here discards them for the application.
        let mut prefix = b"\x1b[<".to_vec();
        prefix.extend(std::iter::repeat_n(b'1', 10_000));
        input.feed(&prefix);
        assert_eq!(input.buffered_bytes(), prefix.len());
        assert_eq!(input.next(), None);
        assert_eq!(input.buffered_bytes(), prefix.len());

        // Draining does not help: every `next()` returns `None` and leaves the
        // count where it was, so only the application can decide what a growing
        // buffer means.
        for _ in 0..3 {
            assert_eq!(input.next(), None);
            assert_eq!(input.buffered_bytes(), prefix.len());
        }
    }

    #[test]
    fn test_input_decoder_recovers_from_unterminated_mouse_prefix() {
        let mut input = InputDecoder::new();

        // A byte that cannot occur in the parameters of an SGR sequence means the
        // sequence can never be valid. The `ESC [ <` marker is reported as
        // undecodable, and the bytes after it are parsed as the ordinary input
        // they turned out to be instead of being swallowed with it.
        input.feed(b"\x1b[<12a");
        assert_eq!(
            input.next(),
            Some(Input::Unrecognized {
                bytes: vec![0x1b, b'[', b'<']
            })
        );
        for expected in ["1", "2", "a"] {
            assert_eq!(
                input.next(),
                Some(Input::Key(KeyInput {
                    ctrl: false,
                    alt: false,
                    code: KeyCode::Char(expected.parse().expect("ascii digit or letter")),
                }))
            );
        }
        assert_eq!(input.next(), None);
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
    /// control characters, arrow keys, function keys, special keys, modified
    /// keys, mouse sequences, UTF-8, unknown sequences, and incomplete
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
        b"\x1b[15~",
        b"\x1b[1;5A",
        b"\x1b[3;5~",
        b"\x1b[<0;10;5M",
        b"\x1b[M!\x2b\x26",
        "\u{3042}".as_bytes(),
        b"\x1b[?25l",
        b"\x1b[9~",
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
            KeyCode::F(n) if !ctrl && !alt => {
                let n = match n {
                    1 => 11,
                    2 => 12,
                    4 => 14,
                    5 => 15,
                    6 => 17,
                    7 => 18,
                    8 => 19,
                    9 => 20,
                    10 => 21,
                    11 => 23,
                    12 => 24,
                    _ => return None,
                };
                let mut v = Vec::new();
                v.extend_from_slice(n.to_string().as_bytes());
                v.push(b'~');
                Some(esc(&v))
            }
            KeyCode::F(n) => {
                let n = match n {
                    1 => 11,
                    2 => 12,
                    4 => 14,
                    5 => 15,
                    6 => 17,
                    7 => 18,
                    8 => 19,
                    9 => 20,
                    10 => 21,
                    11 => 23,
                    12 => 24,
                    _ => return None,
                };
                let m = 1 + if alt { 2 } else { 0 } + if ctrl { 4 } else { 0 };
                let mut v = Vec::new();
                v.extend_from_slice(n.to_string().as_bytes());
                v.push(b';');
                v.push(b'0' + m);
                v.push(b'~');
                Some(esc(&v))
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
                // Bytes are consumed only together with an event: the decoder
                // settles what it cannot decode instead of dropping it.
                let input = input.expect("consumed bytes must come with an event");
                rest = &rest[consumed..];
                if matches!(input, Input::Unrecognized { .. }) {
                    expected_unknown = true;
                }
                expected.push(input);
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
