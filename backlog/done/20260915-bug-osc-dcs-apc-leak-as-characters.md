# Bug: OSC, DCS, and APC bodies spill out as characters

- Status: fixed

## Summary

A control string introduced by `ESC ]` (OSC), `ESC P` (DCS), or `ESC _` (APC)
is not decoded at all. The decoder consumes only the two-byte introducer as
`Alt` plus that character and then returns the whole body — a title, a device
control payload, or a base64 image — as a stream of `Input::Key` characters.

APC is what makes this worth fixing rather than tolerating: the kitty graphics
protocol sends each image as `ESC _ G ... ESC \`, so a single graphic reaches
the application as tens of thousands of key events.

## Reproduction

Feed an OSC sequence and drain the decoder:

```text
input:  ESC ] 0 ; h i BEL
calls:  d.feed(b"\x1b]0;hi\x07"); while let Some(i) = d.next() { ... }

observed events:
  Input::Key(Alt+']')
  Input::Key('0')
  Input::Key(';')
  Input::Key('h')
  Input::Key('i')
  Input::Key(Char(0x07))   # the BEL terminator, as a character
```

The bodies of `ESC P ... ESC \` and `ESC _ ... ESC \` behave the same way, only
the introducer character differs.

The split does not matter: the first event appears as soon as two bytes have
arrived, and the rest follow one byte per `next()`.

## Observed behavior

`parse_escape_sequence` in `src/input.rs` dispatches on `bytes[1]` with three
arms — `b'['` to `parse_csi_sequence`, `b'O'` to `parse_ss3_sequence`, and

```rust
b if b < 0x80 && b != 0x1b && b != 0x5b && b != 0x4f => parse_alt_char(bytes),
```

for everything else printable. `]` (0x5D), `P` (0x50), and `_` (0x5F) all
match that last arm, so `parse_alt_char` reads `bytes[1]` and returns
`(Some(create_key_input(.., alt = true, ..)), 2)`. It never looks for a
terminator, so the body is left in the buffer to be decoded as ordinary input.

This contradicts the contract stated on `InputDecoder::next`:

> A sequence the decoder cannot decode is not held back: its bytes are consumed
> and returned as [`Input::Unrecognized`]. So `None` never means "bytes were
> dropped"; a caller that wants to notice a mis-decode gets it as a value.

The bytes are not reported as `Unrecognized` and they are not reported as
nothing; they are reported as keys the user never pressed.

## Expected behavior

A control string the decoder does not interpret should be settled as one
[`Input::Unrecognized`] value covering the whole string, the way an unknown CSI
sequence already is. The decoder does not need to parse OSC or DCS payloads to
do this; it only needs to find the terminator, which is defined:

- OSC is terminated by `BEL` (0x07) or by `ST` (`ESC \`).
- DCS and APC are terminated by `ST` (`ESC \`).

Consuming up to and including the terminator, as `find_parameter_terminator`
does for unknown CSI sequences, would keep the body from reaching the
application as characters.

## Impact

Correctness, reachable from the public API: an application receives input that
the user did not type, and the amount is unbounded. A title-setting OSC from a
shell prompt or a program in an alternate screen produces a handful of spurious
keys; an APC graphic produces as many events as the image has bytes, so a
caller that processes every event will stall on a single graphic.

The buffer does not grow in this case, because `parse_alt_char` settles two
bytes at a time. The damage is to the event stream, not to memory.

## Notes

- The terminator scan has to handle the `ST` form (`ESC \`), not only `BEL`,
  because APC and DCS have no other terminator.
- Related: this issue and the missing 8-bit CSI handling filed next to it are
  the two shapes of "input the decoder does not know but does not say so". The
  fix for either should not reintroduce the other; a shared notion of "consume
  through the terminator and report it" covers both.
- Bracketed paste (`ESC [ 200 ~` ... `ESC [ 201 ~`) is a different case with a
  different cure (recognizing the pair), and is not part of this bug.

## Outcome

Fixed in PR #36 (merge `3e70a38`).

- A control string is now scanned to its terminator and settled as one
  `Input::Unrecognized` covering the whole sequence. The scan accepts `BEL`
  (0x07) or `ST` (`ESC \`), and accepts `BEL` only for OSC, since DCS and APC
  have no other terminator.
- An unterminated control string returns `(None, 0)`: it is genuinely
  incomplete, so the bytes stay buffered and `None` keeps its meaning of "wait
  for more input". A control string that a caller wants to abandon before its
  terminator arrives is the case the bounded-incomplete-sequence RFC covers.
- `Alt+]`, `Alt+P`, and `Alt+_` are no longer expressible, because the decoder
  cannot tell them from an introducer. An application that wants them reads the
  bytes back from `Input::Unrecognized`, whose documentation now states that
  the payload is passed through unmodified.
- The scan reads past an `ESC` that does not start `ST`, so a body may contain
  escape bytes without ending the string early.
