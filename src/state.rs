use std::io::Write;

use crate::{TerminalFrame, TerminalInput, TerminalPosition, TerminalSize, input::InputBuffer};

/// The I/O-free core state of a terminal application.
///
/// This type owns the data that an application keeps between frames and inputs:
/// the terminal size, the last frame that was rendered, the cursor position, and
/// the buffer of unparsed input bytes. It performs no I/O of its own.
///
/// It is paired with a driver (see [`TerminalDriver`](crate::TerminalDriver)) that
/// owns the file descriptors and terminal modes. The application reads raw bytes
/// from the driver, feeds them into [`TerminalState::push_input()`], and writes
/// the bytes produced by [`TerminalState::render()`] to the driver.
pub struct TerminalState {
    size: TerminalSize,
    last_frame: TerminalFrame,
    cursor: Option<TerminalPosition>,
    input: InputBuffer,
}

impl TerminalState {
    /// Makes a new, empty state for a terminal of the given size.
    pub fn new(size: TerminalSize) -> Self {
        Self {
            size,
            last_frame: TerminalFrame::new(size),
            cursor: None,
            input: InputBuffer::default(),
        }
    }

    /// Returns the current terminal size.
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Updates the terminal size.
    pub fn set_size(&mut self, size: TerminalSize) {
        self.size = size;
    }

    /// Returns the cursor position that will be applied by the next
    /// [`TerminalState::render()`](Self::render), or `None` when the cursor is
    /// hidden.
    pub fn cursor(&self) -> Option<TerminalPosition> {
        self.cursor
    }

    /// Sets the cursor position to be shown by the next
    /// [`TerminalState::render()`](Self::render).
    pub fn set_cursor(&mut self, position: Option<TerminalPosition>) {
        self.cursor = position;
    }

    /// Appends raw input bytes to the internal buffer.
    pub fn push_input(&mut self, bytes: &[u8]) {
        self.input.push(bytes);
    }

    /// Parses and returns the next complete input event.
    ///
    /// Returns `None` when the buffer does not yet contain a complete event (for
    /// example a lone `ESC` byte), which stays buffered until the rest of the
    /// sequence arrives. Call [`TerminalState::has_pending_input()`](Self::has_pending_input)
    /// to check whether such an incomplete sequence is being held.
    pub fn next_input(&mut self) -> Option<TerminalInput> {
        self.input.next()
    }

    /// Returns `true` when the buffer holds unconsumed bytes.
    pub fn has_pending_input(&self) -> bool {
        !self.input.is_empty()
    }

    /// Renders a frame into `out`, clearing `out` first.
    ///
    /// The output is the byte sequence that hides the cursor, re-draws only the
    /// lines that differ from the previous frame, and positions / shows the
    /// cursor. The caller writes these bytes to the driver and flushes it.
    pub fn render(&mut self, frame: TerminalFrame, out: &mut Vec<u8>) {
        out.clear();
        let _ = write!(out, "\x1b[?25l"); // hide cursor

        let resized = self.last_frame.size() != frame.size();
        let mut skipped = false;
        let mut last_style = None;
        let mut last_row = usize::MAX;
        for (position, c) in frame.chars() {
            let old = self.last_frame.get_char(position);
            if !resized && Some(c) == old {
                skipped = true;
                continue;
            }

            if skipped || last_row != position.row {
                let _ = write!(out, "\x1b[{};{}H", position.row + 1, position.col + 1);
            }
            if Some(c.style()) != last_style {
                let _ = write!(out, "{}", c.style());
            }
            let _ = write!(out, "{}", c.value());

            last_style = Some(c.style());
            last_row = position.row;
            skipped = false;
        }

        if let Some(position) = self.cursor {
            let _ = write!(out, "\x1b[{};{}H", position.row + 1, position.col + 1);
            let _ = write!(out, "\x1b[?25h"); // show cursor
        }

        self.last_frame = frame;
    }
}
