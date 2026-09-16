# Input Decoding

This document is the reference for what
[`InputDecoder`](crate::InputDecoder) recognizes: the byte sequences that
become an [`Input`](crate::Input) value, and what happens to the bytes that
become nothing.

For how to drive the decoder (feeding bytes, draining events, bounding the
buffer, resolving a lone `ESC`), see the [`InputDecoder`](crate::InputDecoder)
rustdoc and [demo.rs][demo]. This document answers the other question: given a
byte sequence, what does tuinix read it as?

[demo]: https://github.com/sile/tuinix/blob/main/examples/demo.rs

## The shape of the input

Everything is matched against a byte stream, not against a terminal's state.
The decoder has no way to ask which modes an application turned on, so the same
bytes mean the same thing in every context: what is decoded is exactly what the
bytes say.

Bytes are taken from the front of the buffer and either settle into an
[`Input`](crate::Input) or stay buffered as an incomplete prefix. Once a prefix
settles, its bytes are gone; once it is incomplete, none of it is reported yet.
A prefix that can never be completed does not exist — every prefix either
settles or is waiting for a byte that can still arrive.

The `ESC X` form and the one-byte C1 form are the same sequence written two
ways, and tuinix reads them the same. Every table below lists the `ESC X`
spelling; substitute the equivalent C1 byte from this table and the decoding is
unchanged.

| Introducer | C1 byte | Sequence |
| ---------- | ------- | -------- |
| `ESC [`   | `0x9b` | CSI: the general "control sequence" form below |
| `ESC ]`   | `0x9d` | OSC: a control string |
| `ESC P`   | `0x90` | DCS: a control string |
| `ESC _`   | `0x9f` | APC: a control string |
| `ESC O`   | `0x8f` | SS3: not supported, and the C1 form is not recognized at all |

## Characters and keys

| Bytes | Input |
| ----- | ----- |
| `0x01..=0x1f`, except `0x09`, `0x0d`, `0x1b` | Ctrl+letter (`0x01` is Ctrl+A). `0x08` is read as Ctrl+H, not Backspace |
| `0x09` | Tab |
| `0x0d` | Enter |
| `0x1b` followed by a byte that introduces nothing below | Alt+that character. An `ESC` before a control byte is Alt+that control key |
| `0x1b` alone | Held, or the Escape key once committed (see below) |
| `0x20..=0x7e` | That character |
| `0x7f` | Backspace |
| UTF-8 lead byte `0xc2..=0xf4` | The decoded character, once its continuation bytes have arrived |

A lone `ESC` is ambiguous: a terminal sends the same byte whether the user
pressed the Escape key or started a sequence such as `ESC [ A`. The decoder
holds it until more input decides the question, and
[`commit_escape()`](crate::InputDecoder::commit_escape) is how an application
resolves it the other way after a short timeout. Only a lone `ESC` is held that
way; every other incomplete prefix is waiting for a byte that is simply not
there yet.

## CSI: `ESC [`

`ESC [` introduces a control sequence. `A`, `B`, `C`, and `D` are the four
arrow directions and take no parameters; anything else here is read as
parameters, a terminator, or both.

### Keys with a fixed shape

| Sequence | Key |
| -------- | --- |
| `ESC [ A` (also `B`, `C`, `D`) | Up, Down, Right, Left |
| `ESC [ H` | Home |
| `ESC [ F` | End |
| `ESC [ Z` | BackTab (Shift+Tab) |
| `ESC [ <digit> A..D` | The same arrow. The row count is discarded: `ESC [ 5 A` is Up, not "up five rows" |
| `ESC [ 1 ; <mod> A..D` | That arrow, with the modifiers below |

### Keys with a `~` terminator

The number before the `~` is an xterm key number, and the numbering is not an
arithmetic sequence — `13`, `16`, and `22` stand for no key here, so anything
not listed settles as undecodable input.

| Sequence | Key |
| -------- | --- |
| `ESC [ 1 ~` or `ESC [ 7 ~` | Home |
| `ESC [ 2 ~` | Insert |
| `ESC [ 3 ~` | Delete |
| `ESC [ 4 ~` or `ESC [ 8 ~` | End |
| `ESC [ 5 ~` | PageUp |
| `ESC [ 6 ~` | PageDown |
| `ESC [ 11 ~` | F1 |
| `ESC [ 12 ~` | F2 |
| `ESC [ 14 ~` | F4 |
| `ESC [ 15 ~` | F5 |
| `ESC [ 17 ~` | F6 |
| `ESC [ 18 ~` | F7 |
| `ESC [ 19 ~` | F8 |
| `ESC [ 20 ~` | F9 |
| `ESC [ 21 ~` | F10 |
| `ESC [ 23 ~` | F11 |
| `ESC [ 24 ~` | F12 |

The `~` form of Home, End, Insert, and Delete also accepts a modifier:
`ESC [ 1 ; <mod> ~` and so on. The function key number and the modifier may be
written together, as in `ESC [ 15 ; 2 ~` for a modified F5.

### Modifiers

The `<mod>` parameter is the number xterm sends: bit `0x2` is Alt and bit
`0x4` is Ctrl, so `1` is no modifier, `3` is Alt, `5` is Ctrl, and `7` is both.
Bit `0x1` is Shift, which [`KeyInput`](crate::KeyInput) has no room for; it is
ignored rather than reported, so `ESC [ 15 ; 2 ~` reads as Alt+F5.

### Terminal reports and other CSI sequences

Everything else that starts with `ESC [` is consumed whole and reported as
[`Input::Unrecognized`](crate::Input::Unrecognized). This covers the reports a
terminal sends in reply to what the application wrote — `ESC [ ? 25 l` for the
cursor, `ESC [ 2 J` to clear the screen, `ESC [ 1 P` to delete a line — and any
sequence tuinix does not decode. The bytes are handed over as they arrived, so
an application that wants one of them can read it out of the report. A
terminfo-driven application that wants to write such a sequence does not use
the decoder at all: it writes the bytes to the terminal directly.

## Mouse reports

Mouse input arrives in two shapes. Which one a terminal sends is decided by
the application's reporting mode, so the decoder accepts both at once.

| Sequence | Meaning |
| -------- | ------- |
| `ESC [ < <button> ; <col> ; <row> M` | SGR mode: a press, a release, a drag, or a wheel event. The coordinates are 1-based |
| `ESC [ < <button> ; <col> ; <row> m` | The same, for a release, when the terminal separates the two |
| `ESC [ M <button> <col> <row>` | X10/X11 mode: three payload bytes, each carrying the value plus 32. Decoding subtracts 33, so the position is 1-based |

In X10/X11 mode each coordinate is a single byte, so it cannot exceed what a
byte holds after the encoding's offset is removed: 223 columns or rows, and a
taller or wider terminal reports a position that has wrapped. SGR mode has no
such limit.

The button number encodes the modifiers: `0x04` is Shift, `0x08` is Alt, and
`0x10` is Ctrl. The rest of the number says what happened and which button.

## Control strings: `ESC ]`, `ESC P`, `ESC _`

OSC, DCS, and APC carry an opaque body whose length is not known in advance, so
the decoder's only job is to find the end of it. A complete control string is
consumed whole and reported as
[`Input::Unrecognized`](crate::Input::Unrecognized); the body is never read as
input, because a body is data rather than keystrokes. This is what keeps an
image sent over APC (the kitty graphics protocol) from arriving as thousands of
key events.

| Sequence | Ends at |
| -------- | ------- |
| `ESC ] ... BEL` | `0x07` |
| `ESC ] ... ESC \` | the two-byte string terminator |
| `ESC P ... ESC \` | the two-byte string terminator |
| `ESC _ ... ESC \` | the two-byte string terminator |

Whichever terminator comes first ends the string. Until one does, nothing is
reported: the decoder waits for the end rather than guessing at a body length.

## Undecodable input

The decoder never silently drops bytes. Anything it consumes without producing
a key or mouse event is reported as
[`Input::Unrecognized`](crate::Input::Unrecognized), whose payload is the bytes
exactly as they arrived.

This is what happens to:

- a control string, as above;
- a CSI sequence with a terminator tuinix does not map to a key;
- an SS3 sequence (`ESC O`) with a final byte tuinix does not know;
- a byte that is not a valid UTF-8 lead or continuation byte;
- a byte in the C1 range that is not one of the introducers above.

Two consequences are worth knowing before relying on it.

**Some Alt combinations cannot be typed.** Where a byte has two readings, the
standard's meaning wins: `ESC ]` is an OSC introducer rather than Alt+`]`,
`ESC P` and `ESC _` likewise, and `Alt+[` cannot be expressed because `0x9b`
is read as CSI in its own right. In each case the bytes still reach the
application, as the payload of the report, so a caller that wants the other
reading can take it from there.

**Nothing is bounded.** A payload may be as long as the sequence that arrived
(a full image, for instance), and an incomplete sequence sits in the buffer
until it is completed or trimmed. The application decides how much to keep; see
the [`InputDecoder`](crate::InputDecoder) rustdoc for the shape of such a loop
and for [`trim_buffered_bytes()`](crate::InputDecoder::trim_buffered_bytes),
which sheds the oldest bytes when the buffer is too long.

## Not supported

These are not read as input today. They are listed so the gap is not mistaken
for a decode that merely failed:

- SS3 (`ESC O`) is recognized only for the arrow, Home, and End keys; the F1–F4
  keys many terminals send that way are undecodable input here, and the C1 form
  (`0x8f`) is not recognized at all.
- Bracketed paste (`ESC [ 200 ~` and `ESC [ 201 ~`) is undecodable input, and a
  paste arrives as the characters between the two markers.
- The focus events (`ESC [ I`, `ESC [ O`) are undecodable input. They are CSI
  sequences like any other, so they settle as reports rather than keys.
- `ESC [ 13 ~`, F3 on the terminals that send it, is undecodable input.
