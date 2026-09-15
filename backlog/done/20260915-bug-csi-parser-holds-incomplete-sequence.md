# Bug: CSI parser holds an incomplete sequence forever

- Status: fixed

## Summary

A CSI sequence that starts with a digit `1`..=`6` but is shorter than six
bytes is never consumed, so `InputDecoder::next()` keeps returning `None` while
the bytes stay in the buffer. `ESC[1A` is the smallest reproduction: it is a
complete, well-formed sequence that the decoder neither handles nor discards.

Any count `1`..=`6` shows the stall, not just `1`: `ESC[5A` is held exactly the
same way. The sequences are well-formed, not merely unrecognized. `CSI A` and
`CSI 1 A` both mean "cursor up one line", and `CSI 5 A` means "cursor up five
lines", so all of them are known arrow sequences; the decoder already handles
the modified form `ESC[1;5A`. It is not declining unknown sequences, it is
failing to recognize known ones.

## Reproduction

Feed the decoder the four bytes `ESC [ 1 A`:

```text
feed \x1b[1A
next()  -> None
next()  -> None
...
```

`buffered_bytes()` stays at `4` and never shrinks. The same holds for any
single count digit: `\x1b[5A` also stalls with four bytes buffered. Feeding
`\x1b[1;5A` (modified arrow, six bytes) succeeds, and `\x1b[X` (unknown
terminator) is consumed as three bytes, so the failure is specific to the
"starts with `1`..=`6`, total length under six" shape. The same input split so
the decoder sees `\x1b[` and then `A` produces a normal `Up` key, which is the
behavior the whole sequence should have had.

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
- Any count digit `1`..=`6` reaches `parse_complex_csi_key`, so the gap is not
  specific to `ESC[1A`; every digit-prefixed arrow shorter than six bytes is
  held.
- The `next() -> None` contract cannot distinguish "waiting for more bytes"
  from "these bytes can never decode", and `buffered_bytes()` is the only
  signal a caller can compare across calls.

## Expected behavior

A single-digit count followed by an arrow final should decode to the matching
arrow: `ESC[<d>A` -> `KeyCode::Up`, `B` -> `Down`, `C` -> `Right`, `D` ->
`Left`, for every digit `d` in `1`..=`6`. `ESC[1A` is therefore `Up`, the same
as `ESC[A`, and `ESC[5A` is `Up` too.

The count is discarded. tuinix maps arrows to `KeyCode::{Up, Down, Left,
Right}`, which carry no magnitude, so "up five lines" cannot be represented as
a single key event. Discarding the count is what the decoder already does for
the modified form `ESC[1;5A`, and it keeps `ESC[<d>A` consistent with `ESC[A`
for every `d`. A complete sequence must never be held for more input, so
neither `ESC[1A` nor `ESC[5A` should pin the buffer.

The fix cannot simply drop the bytes: `CSI A` and `CSI 1 A` are equivalent, so
treating `ESC[1A` as unrecognized would make the decoder inconsistent with its
own handling of `ESC[A`. Collapsing to a single arrow event is the option that
leaves no input in the "starts with a digit, under six bytes" shape stuck.

## Impact

A caller that decodes from a `Read` source can take unexpected bytes (for
example the output of `yes`, or another program's ANSI output) and grow its
input buffer without bound, because the decoder never advances. This is a
correctness problem, not an ergonomics one: the same input is decodable when
fed in a different split, so the decoder is the component at fault.

## Notes

- `ESC[<d>A` and `ESC[1;5A` go through the same state machine
  (`parse_complex_csi_key`); the count-then-final shape and the
  digit-semicolon-digit shape should be handled together, not patched twice. The
  shared cause is that the fallback asks for six bytes even for shapes that are
  only four bytes long.
- `next() -> None` is documented as the result of an *incomplete* sequence, and
  a complete sequence is by definition not incomplete, so the observed behavior
  contradicts the method's own contract rather than merely being inconvenient.
- The count is a distance for the cursor to move (for example `CSI 5 A` moves
  five lines), but a key event cannot carry a magnitude, so it is discarded and
  the digit is accepted for consistency with `ESC[A`. Preserving the count would
  need a larger `KeyCode` (or a new event kind) and is an API question, not part
  of this fix.
- This fix is scoped to arrows (`A`..=`D`), which is what reaches
  `parse_complex_csi_key` from the digit dispatch. The same dispatch also sends
  `1`..=`6` there for the `~` family, which has a related but distinct defect:
  `ESC[15~` falls through to the fallback and consumes only three bytes, leaking
  `5~` as two characters. That is filed separately; do not widen this fix to
  cover it.
- A richer return type that reports an undecodable item would be an API change;
  this bug does not need one, because `KeyCode::Up` already exists. If a future
  change wants to report discarded input, it should argue that in an RFC and
  keep this report as the reproduction.
