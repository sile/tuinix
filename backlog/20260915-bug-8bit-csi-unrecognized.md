# Bug: 8-bit CSI (`0x9B`) is not recognized as a control sequence

- Status: open

## Summary

C1 control codes are not handled. A terminal that sends the 8-bit form of CSI
as a single byte `0x9B` instead of the two-byte `ESC [` is not understood: the
byte starts a UTF-8 character instead of a control sequence, so the parameter
and terminator bytes that follow are reported as characters.

## Reproduction

Feed the 8-bit form of `ESC [ A` (Cursor Up) and drain the decoder:

```text
input:  0x9B 'A'
calls:  d.feed(&[0x9b, b'A']); while let Some(i) = d.next() { ... }

observed events:
  Input::Unrecognized { bytes: [0x9b, b'A'] }
```

The two bytes are reported as undecodable rather than as Cursor Up. Because
`0x9B` is treated as the lead byte of a UTF-8 character, the reported length
depends on the bytes that follow: a valid-looking continuation sequence is
folded into the character rather than stopping at the terminator.

## Observed behavior

`parse_input` in `src/input.rs` dispatches on `bytes[0]` with

```rust
b if b < 0x80 && b != 0x1b && b != 0x7f => parse_ascii_char(bytes),
0x1b => parse_escape_sequence(bytes),
0x7f => (Some(create_key_input(false, false, KeyCode::Backspace)), 1),
b if b >= 0x80 => parse_utf8_char(bytes),
```

so every byte at or above `0x80`, including the C1 range `0x80..=0x9F`, goes to
`parse_utf8_char`. `0x9B` is not a valid UTF-8 lead byte, so the input is
settled as `Input::Unrecognized`. Nothing in `src/` compares against `0x9b`,
and C1 control codes in general (for example `0x9B`, `0x9D`, `0x90`) are not
recognized.

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
- Explicitly settle the whole C1 range as `Unrecognized` with the correct
  length rather than letting it be decided by UTF-8 continuation rules, so at
  least the boundaries are honest.

Either way `0x9B 'A'` should not be reported as a two-byte undecodable blob
because of UTF-8 rules that do not apply to it.

## Impact

Narrow. Modern terminals in UTF-8 mode send the two-byte `ESC [` form, so most
applications never see `0x9B`. It matters for a terminal or a byte stream in
8-bit control mode, and for correctness of the "C1 is not text" boundary.

## Notes

- If this becomes C1-aware, the same terminator question as the OSC/DCS/APC
  bug applies to `0x9D` and `0x90`.
- The minimum fix that preserves today's behavior is to keep reporting
  `Unrecognized` but stop letting `parse_utf8_char` decide the length for bytes
  in `0x80..=0x9F`.
- Worth checking whether the UTF-8 path can currently consume a following
  printable byte as a continuation and so hide two inputs in one report; that
  is a separate question from C1, but the same reproduction would show it.
