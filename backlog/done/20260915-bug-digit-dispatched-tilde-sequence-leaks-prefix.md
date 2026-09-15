# Bug: a digit-dispatched `~` sequence consumes only three bytes and leaks its tail

- Status: fixed

## Summary

A CSI sequence that starts with `1`..=`6` and ends in `~` is routed to the
complex path, which does not recognize the two-digit form. When the shape is
not one it handles, the fallback consumes three bytes, so everything after
`ESC [` is re-decoded as ordinary characters. `ESC[15~` (a common F-key
encoding) leaks `5` and `~` as two key events instead of producing one key.

This is a bug, not an RFC: the decoder reports a complete sequence as
unknown, drops the prefix, and emits output that does not correspond to the
input, without any documented contract saying a known sequence may be split
like that.

## Reproduction

Feed the decoder six bytes `ESC [ 1 5 ~`:

```text
feed \x1b[15~
next()  -> None            (fallback consumed 3 bytes)
next() -> Char('5')
next() -> Char('~')
```

The first `next()` reports three bytes consumed; `buffered_bytes()` drops from
`6` to `3`, and the remaining bytes surface as characters. The behavior is
specific to the digit dispatch: `\x1b[5~` (`bytes[2] == b'5'`, also digit
routed) reaches `parse_special_key_simple` because it is four bytes and
`bytes[3] == b'~'`, and decodes to `PageUp`. `\x1b[15~` is six bytes and so
misses that branch, lands in the fallback, and is consumed as unknown.

## Observed behavior

- `parse_csi_sequence` dispatches `bytes[2]` in `b'1'..=b'6'` to
  `parse_complex_csi_key`, which only understands:

  ```rust
  // ESC [ 3 ~                        (four bytes)
  if bytes.len() >= 4 && bytes[3] == b'~' { ... }
  // ESC [ 3 ; 5 ~                    (six bytes)
  if bytes.len() >= 6 && bytes[3] == b';' && bytes[5] == b'~' { ... }
  ```

  A two-digit number (`ESC [ 1 5 ~`) matches neither, so control reaches the
  fallback and `bytes.len() < 6` is false, giving `(None, 3)`.
- `parse_special_key_simple` and `parse_special_key_with_modifier` in fact table
  digits `1`..=`8` (`b'7' | b'1' => Home`, and so on), but only the single-digit
  speller reaches them, so the table is partially unreachable.
- The `next() -> None` contract again cannot distinguish "waiting" from
  "decided to skip": the return is the same shape as the incomplete case, and
  the only difference a caller can observe is that `buffered_bytes()` fell.

## Expected behavior

`ESC[15~` should decode as one key event (the function-key family anchored at
`15`), consuming all six bytes, or, if the decoder deliberately does not support
that key, it should be discarded whole rather than partially. Silently
discarding three bytes and re-reading the rest as characters is the part that
is not defensible: the input is a complete, well-formed CSI sequence and the
decoder does not consume it as one.

The fix should decide the full set of supported digit counts and route the
two-digit form to the same table that already holds the `~` names, so a single
sequence produces a single event or is dropped in full.

## Impact

A caller reading a `Read` source (a terminal's ANSI output, a program emitting
function keys) receives spurious `Char` events for inputs that are legitimate
key sequences. Unlike the arrow stall (see
`20260915-bug-csi-parser-holds-incomplete-sequence.md`), the buffer does not
grow without bound, so this is a mis-decode rather than a resource leak; but it
is still a correctness problem, and the fix belongs to the same dispatch table.

## Notes

- Related to `20260915-bug-csi-parser-holds-incomplete-sequence.md` by shared
  cause: `parse_csi_sequence` sends every `b'1'..=b'6'` to
  `parse_complex_csi_key`, but that function's branches are written for specific
  lengths and digit counts. Fixing the arrow case does not fix this one, and
  vice versa; each should be settled on its own with its own test.
- Which `~` counts are real keys is itself a decision. `2 ~` / `3 ~` / `4 ~` /
  `5 ~` / `6 ~` / `7 ~` / `8 ~` are the classic screen/keypad block, `15 ~`..`21 ~`
  cover F5..F12 on many terminals, and others exist. Choosing the supported set,
  and whether an unsupported count is dropped whole or surfaced, is what the fix
  has to answer; the bug is that the current code answers it by accident, three
  bytes at a time.
