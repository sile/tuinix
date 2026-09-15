# Bug: CSI parser holds an incomplete sequence forever

- Status: open

## Summary

A CSI sequence that starts with a digit but is shorter than six bytes is never
consumed, so `InputDecoder::next()` keeps returning `None` while the bytes stay
in the buffer. `ESC[1A` is the smallest reproduction: it is a complete,
well-formed sequence that the decoder neither handles nor discards.

The sequence is well-formed, not merely unrecognized: `CSI A` and `CSI 1 A`
both mean "cursor up one line", and the decoder already handles the modified
form `ESC[1;5A`. So the decoder is not declining an unknown sequence; it is
failing to recognize a known one.

## Reproduction

Feed the decoder the four bytes `ESC [ 1 A`:

```text
feed \x1b[1A
next()  -> None
next()  -> None
...
```

`buffered_bytes()` stays at `4` and never shrinks. Feeding `\x1b[1;5A`
(modified arrow, six bytes) succeeds, and `\x1b[X` (unknown terminator) is
consumed as three bytes, so the failure is specific to the "starts with a
digit, total length under six" shape. The same input split so the decoder sees
`\x1b[` and then `A` produces a normal `Up` key, which is the behavior the
whole sequence should have had.

## Observed behavior

- `parse_complex_csi_key` in `src/input.rs` only reaches its fallback after its
  shape checks, and the fallback is:

  ```rust
  if bytes.len() < 6 {
      (None, 0)   // wait for more input
  } else {
      (None, 3)   // consume as unknown
  }
  ```

  So a buffer that already holds a complete sequence is reported as "need more
  input".
- The `next() -> None` contract cannot distinguish "waiting for more bytes"
  from "these bytes can never decode", and `buffered_bytes()` is the only
  signal a caller can compare across calls.

## Expected behavior

`ESC[1A` should decode to `KeyCode::Up`, the same as `ESC[A` and `ESC[1;5A`.
The emitted line count is discarded, matching how the decoder already treats
the modified form. A complete sequence must never be held for more input, so
`ESC[1A` should not pin the buffer indefinitely.

The fix cannot simply drop the bytes: `CSI A` and `CSI 1 A` are equivalent, so
treating `ESC[1A` as unrecognized would make the decoder inconsistent with its
own handling of `ESC[A`.

## Impact

A caller that decodes from a `Read` source can take unexpected bytes (for
example the output of `yes`, or another program's ANSI output) and grow its
input buffer without bound, because the decoder never advances. This is a
correctness problem, not an ergonomics one: the same input is decodable when
fed in a different split, so the decoder is the component at fault.

## Notes

- `ESC[1A` and `ESC[1;5A` go through the same state machine
  (`parse_complex_csi_key`); the one-byte-and-digit shape and the
  digit-semicolon-digit shape should be handled together, not patched twice.
- `next() -> None` is documented as the result of an *incomplete* sequence, and
  a complete sequence is by definition not incomplete, so the observed behavior
  contradicts the method's own contract rather than merely being inconvenient.
- The number in a digit-prefixed arrow is a count for how far the cursor moves
  (for example `CSI 5 A`); tuinix maps arrows to `KeyCode::{Up, Down, Left,
  Right}` without a magnitude, so the count is discarded. Whether every digit
  `1`..=`6` should map to an arrow, or only `1`, is a detail to settle with the
  fix.
- A richer return type that reports an undecodable item would be an API change;
  this bug does not need one, because `KeyCode::Up` already exists. If a future
  change wants to report discarded input, it should argue that in an RFC and
  keep this report as the reproduction.
