# Bug: 8-bit CSI (`0x9B`) is not recognized as a control sequence

- Status: fixed

## Summary

C1 control codes are not handled. A terminal that sends the 8-bit form of CSI
as a single byte `0x9B` instead of the two-byte `ESC [` is not understood: the
byte is reported as a one-byte undecodable value, and the bytes that follow it
are then read as ordinary input. A Cursor Up arrives as "one unknown byte"
followed by the character `A`, and the two spellings of the same key do not
produce the same event.

## Reproduction

Feed the 8-bit form of `ESC [ A` (Cursor Up) and drain the decoder:

```text
input:  0x9B 'A'
calls:  d.feed(&[0x9b, b'A']); while let Some(i) = d.next() { ... }

observed events:
  Input::Unrecognized { bytes: [0x9b] }
  Input::Key(KeyInput { code: KeyCode::Char('A'), .. })
```

Measured, not inferred: `parse_input` on `[0x9b, b'A']` returns
`(Some(Unrecognized { bytes: [0x9b] }), 1)`, and the decoder then yields the
`A` on the following call. The introducer is settled on its own, one byte at a
time, and the rest of the sequence is read as if it were typed.

The reported length is one byte regardless of what follows, because `0x9B` is
not a valid UTF-8 lead byte: the width table in `parse_utf8_char` falls through
to `1`, `from_utf8` on that single byte fails, and the `unrecognized(bytes, 1)`
arm is taken. Continuation bytes are not folded in. The damage is the opposite
one -- the sequence is cut into a stray byte plus ordinary keystrokes.

## Observed behavior

`parse_input` in `src/input.rs` dispatches on `bytes[0]` with

```rust
b if b < 0x80 && b != 0x1b && b != 0x7f => parse_ascii_char(bytes),
0x1b => parse_escape_sequence(bytes),
0x7f => (Some(create_key_input(false, false, KeyCode::Backspace)), 1),
b if b >= 0x80 => parse_utf8_char(bytes),
```

so every byte at or above `0x80`, including the C1 range `0x80..=0x9F`, goes to
`parse_utf8_char`. `0x9B` is not a valid UTF-8 lead byte, so `parse_utf8_char`
settles it as a one-byte `Input::Unrecognized` and reports one byte consumed.
Nothing in `src/` compares against `0x9b`, and C1 control codes in general (for
example `0x9B`, `0x9D`, `0x90`) are not recognized.

This at least stays inside the documented contract on `InputDecoder::next`
(the bytes are reported as a value, not dropped), which is why this is filed as
a small correctness gap rather than a leak like the OSC/DCS/APC case. The
decoded result is still wrong: the sequence is a documented terminal encoding
of a key, and the application gets `Unrecognized` instead.

## Expected behavior

Two coherent options, to be chosen in the fix:

- Treat `0x9B` as CSI, `0x9D` as OSC, and the other C1 introducers as their
  `ESC`-prefixed equivalents, so the existing parsers handle them. This is what
  the C1 set is for in ECMA-48.
- Keep the one-byte report, but make it deliberate: settle the whole C1 range
  in one step instead of routing it through the UTF-8 branch, and document the
  range as undecodable by choice. This is honest about the boundaries without
  making the sequences readable.

Either way `0x9B 'A'` should not arrive as a stray undecodable byte followed by
the character `A`. The first option is the one to take: the C1 introducers
spell sequences tuinix already decodes, and rejecting the table those bytes
come from is not the same as being able to read it.

It was taken. See the Outcome section.

## Impact

Narrow. Modern terminals in UTF-8 mode send the two-byte `ESC [` form, so most
applications never see `0x9B`. It matters for a terminal or a byte stream in
8-bit control mode, and for correctness of the "C1 is not text" boundary.

## Notes

- Making this C1-aware brought in the same terminator question as the
  OSC/DCS/APC bug, for `0x9D` and `0x90`. The fix reaches
  `find_control_string_end` with a one-byte introducer in hand; see the Outcome
  section for how the introducer length is carried.
- The minimum fix that preserves today's behavior is to keep reporting
  `Unrecognized` but settle the whole `0x80..=0x9F` range in one step instead
  of letting the UTF-8 branch decide. That removes the stray-byte-plus-keys
  split only for sequences whose full length is known, so it is the weaker of
  the two options above.

## Outcome

Fixed in PR #37 (merge `7d4e757`). The first of the two options was taken: the
C1 introducers are read as the sequences they spell.

- `0x9B` is CSI, `0x9D` is OSC, `0x90` is DCS, and `0x9F` is APC. Each one
  reaches the same parser as its `ESC`-prefixed spelling and produces the same
  events, so `0x9B 'A'` is Cursor Up rather than a stray byte followed by the
  character `A`.
- The rest of the C1 range (`0x80..=0x9F`) is unchanged: it settles as a
  one-byte `Unrecognized`, which is what `0x9B` used to do.
- The fix did not rebuild an `ESC`-prefixed buffer to feed the existing
  parsers. The parsers now take a `ControlSequence` carrying the original bytes
  plus the index where parameters begin, because the two spellings differ in
  introducer length (two bytes for `ESC`-prefixed, one for C1). Reporting the
  original slice is what keeps the `Unrecognized` payload the bytes that
  arrived, as `#35` requires; a rebuilt buffer would have reported bytes the
  caller never sent.
- `Alt+[` is no longer expressible, for the same reason as `Alt+]` in the
  OSC/DCS/APC fix. An application that wants it reads the bytes back from
  `Input::Unrecognized`.
- The `ControlSequence` change also fixed a length error in the X10 mouse
  parser that the existing tests caught: `ESC [ M` plus three payload bytes is
  six bytes, and the parser had been reporting five.
