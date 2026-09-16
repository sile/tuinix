# RFC: Bound an incomplete sequence without cutting through it

- Status: draft

## Summary

`InputDecoder::trim_buffered_bytes(max_len)` bounds the buffer by discarding
bytes from the front, so when the buffer is one long incomplete sequence it cuts
that sequence in half. The application then sees a fragment of a byte stream the
decoder had already partly interpreted, which is exactly the kind of
half-reported input the crate spent several fixes removing. This RFC proposes a
way to end an abandoned sequence at a known boundary instead of at an arbitrary
byte offset.

## Motivation

The decoder holds bytes for one reason: the bytes so far might still become an
`Input`. Every place that holds is a case where more bytes would let the parser
finish, and the fixes up to now removed the places where that was never true (a
complete sequence held forever). What is left is honest incompleteness: a
sequence whose terminator has not arrived.

There is no bound on how long such a sequence can be. In practice the buffer
grows only for a parameter run that never ends — `ESC [` followed by digits and
semicolons — because any other byte settles the sequence. That is rare, but not
impossible: a line of noise, a mis-sent control sequence, or a stream from a
program that is not speaking the protocol can all open one. A caller that reads
from a device it does not control needs an answer for it.

The answer today is `trim_buffered_bytes(MAX)`. It does bound the buffer, and it
is the only thing that does, but the cut is a byte offset into a stream the
decoder has already classified as "an escape sequence is in progress". The bytes
after the cut are then decoded as ordinary input, so the application gets a
fragment of a sequence as text — the same failure as the leaks this crate fixed,
deliberately chosen. The doc admits this:

> The cut is byte-oriented while the parser is sequence-oriented, so it can land
> in the middle of an incomplete sequence.

This RFC argues the application should not have to make that choice. It should
be able to say "stop waiting for this sequence" and have the decoder discard to
a point it understands, then resume decoding from there.

## Guide-level explanation

An application that reads from a source it does not fully trust bounds the
buffer by ending the sequence rather than by cutting the bytes:

```rust
loop {
    if decoder.buffered_bytes() > MAX_BUFFERED_BYTES {
        decoder.abandon_incomplete_input();
    }
    // ... feed, drain, draw ...
}
```

After the call the decoder holds no bytes from the abandoned sequence, so the
next bytes are decoded from a clean start. The application chooses *when* to
give up (a size bound, a timeout, a user gesture); the decoder chooses *where*
to stop, which is the part it knows and the caller does not.

## Reference-level explanation

The decoder is the only layer that knows where a sequence begins. What this
proposal needs from it is the answer to "if I stop waiting now, what is the
largest prefix I can safely drop?" The natural reading is the bytes of the one
incomplete sequence being held, which is: everything the current buffer holds
that the parser has already committed to as one sequence.

Two shapes are possible for the operation:

- **Abandon and report.** The abandoned bytes are returned as
  `Input::Unrecognized`, matching every other case where the decoder consumes
  bytes it did not turn into a key. The caller can see what was given up on.
- **Abandon and forget.** The bytes are dropped and no value is produced, which
  is what `trim_buffered_bytes` does today.

The first is consistent with the rest of the `Input` surface and makes the
failure visible; the second is simpler and matches the existing "trim" call.
Either way the operation differs from `commit_escape()`: committing settles an
*interpretation* (a lone `ESC` becomes Escape), while abandoning refuses one.
They should stay separate methods.

Nothing here proposes a bound inside the decoder. The decoder would still hold
bytes indefinitely if the application never calls the new operation; the
difference is that when the application does act, the result is a decoded-clean
state rather than a bitten-off fragment.

## Drawbacks

- A second way to shed buffered bytes, next to `trim_buffered_bytes`. Keeping
  both means a caller has to decide which to reach for, and the byte-offset trim
  remains available to produce exactly the fragment this RFC objects to.
- "Where the sequence begins" is only as good as the parser's own boundary. If
the abandoned bytes are the prefix of a legitimate sequence that was merely
slow, the decoder still discards a real sequence — the operation cannot tell
"abandoned" from "slow", and only the caller's policy can.
- Reporting the abandoned bytes as `Unrecognized` creates a large value for a
  large sequence, which is a copy of the very bytes the caller wanted to get rid
  of. "Abandon and forget" avoids that at the cost of silence.

## Rationale and alternatives

**Keep only `trim_buffered_bytes` and document the hazard better.** The smallest
change, and it is rejected: the hazard is not a misunderstanding a doc can
prevent. The method's argument is a byte count, and a byte count that lands
inside a sequence is the operation working as designed. No wording makes the
mid-sequence cut disappear from the API.

**Have the decoder hold a bound** (`with_max_buffered_bytes(n)`). Rejected in
the trim RFC for the same reason it is rejected here: the bound is a policy with
a user-visible trade-off (a real sequence can be cut), and a foundational layer
must not hide that decision. This RFC keeps the decision with the caller and
only makes the action the caller takes a better one.

**A resynchronization state** (the decoder, once told to give up, keeps
consuming until it sees a byte that can start a sequence, e.g. `ESC`). This
turns "abandon" into "abandon and resync", which handles the case where the
junk does not end at a sequence boundary at all. It is worth considering, but it
is a bigger change (the decoder gains a mode) and its correctness depends on
what counts as a resynchronization point. Deferred to a follow-up unless the
simple form proves insufficient.

**Cut at a sequence boundary while keeping the front-drop convention** (a
boundary-aware version of the current method). This fixes the fragment and
leaves the byte-count argument in place, which mostly preserves the awkward
shape; it is a smaller step than this RFC and may be enough if the caller is
happy to pass "how much to keep" in bytes.

## Unresolved questions

- Abandon-and-report (`Input::Unrecognized`) or abandon-and-forget.
- The method name. `abandon_incomplete_input` describes the effect;
  `discard_pending_sequence` and `reset_pending_sequence` are alternatives.
- The exact prefix that is dropped when the buffer holds more than the one
  incomplete sequence (it normally holds only that, because anything settled is
  consumed by `next()`; state that as the precondition or enforce it).
- Whether `trim_buffered_bytes` should remain once this exists, or be narrowed
  to a boundary-aware form.

## Future possibilities

- A resynchronization mode, if abandoning the held sequence alone does not
  bound real streams.
- A timeout helper around `commit_escape()` and this operation, so a caller does
  not reimplement the policy for both.
- Whether the demoted reason for `trim_buffered_bytes` — that it was motivated
  by pastes that grow the buffer — should be corrected in that RFC's history.
  Investigation while writing this found that pastes do not grow the buffer: a
  paste is decoded character by character and the buffer stays small, and an
  OSC/DCS/APC body is likewise consumed two bytes at a time. The buffer grows
  only for an unterminated parameter run.
