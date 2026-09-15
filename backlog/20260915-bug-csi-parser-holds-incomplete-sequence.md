# Bug: CSI parser holds an incomplete sequence forever

- Status: open

## Summary

An unknown CSI sequence that is shorter than six bytes is never consumed, so
`InputDecoder::next()` keeps returning `None` while the bytes stay in the
buffer. `ESC[1A` is the smallest reproduction: it is a complete, well-formed
sequence that the decoder neither handles nor discards.

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
digit, total length under six" shape.

A parser-level test that splits the bytes differently shows the asymmetry: the
same input split so the decoder sees `\x1b[` and then `A` produces a normal
`Key`, while feeding all four bytes at once pins the buffer.

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

A sequence that is complete but unrecognized should be consumed (or reported
as unconsumable), not held. `ESC[1A` should not pin the buffer indefinitely.

## Impact

A caller that decodes from a `Read` source can take unexpected bytes (for
example the output of `yes`, or another program's ANSI output) and grow its
input buffer without bound, because the decoder never advances. This is a
correctness problem, not an ergonomics one: the same input is decodable when
fed in a different split, so the decoder is the component at fault.

## Notes

- The related asymmetry (a one-based arrow such as `ESC[1A` is unsupported
  while `ESC[1;5A` is supported) is the same parser state machine, and the fix
  should be shared rather than patched twice. A real terminal does not emit
  `ESC[1A`, so the asymmetry is reported for the buffer-pinning it causes, not
  as a missing key.
- How to resolve the ambiguity (consume-and-drop versus a richer return type
  that reports an undecodable item) is an API question; if the chosen fix
  changes the public return type it should be argued in an RFC, and this bug
  report kept as the reproduction.
