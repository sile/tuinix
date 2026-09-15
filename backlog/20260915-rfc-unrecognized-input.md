# RFC: Report input the decoder chose to discard

- Status: draft

## Summary

Give `InputDecoder` a way to tell the caller that it consumed bytes it could not
decode, instead of swallowing them. The bytes are already consumed today (the
buffer advances, so nothing grows without bound); what is missing is any signal
that it happened, which makes a mis-decode indistinguishable from silence.

## Motivation

`InputDecoder` has one channel: `next() -> Option<Input>`. `None` means "no
complete input from the bytes fed so far" (see the method's rustdoc). That is
accurate for an incomplete sequence, which will decode once more bytes arrive.
It is not accurate for a sequence the decoder has *decided* it will never
decode: those bytes are dropped, and the caller sees exactly the same `None` it
sees while waiting.

Concretely, `parse_csi_sequence` ends with:

```rust
_ => (None, 3), // Unknown CSI sequence
```

and the same shape appears in `parse_ss3_sequence`, `parse_simple_csi_key`,
`parse_numbered_arrow_key`, `parse_modified_arrow_key`, and at the tail of
`parse_complex_csi_key`. Each of these consumes a fixed number of bytes and
returns no input. `InputDecoder::next()` then loops:

```rust
if input.is_none() && consumed > 0 {
    continue;
}
```

so consumption without input is internal, and the caller learns about it only
by noticing that `buffered_bytes()` fell between two `None` returns. Nothing
in the return values says it.

This matters to an application in two situations.

**Debugging.** A terminal sends an escape sequence the decoder does not know
(a key on a keyboard it was not written for, a terminal that encodes a key
differently than xterm does). The application appears to ignore the key
entirely, with no way to log what arrived. This has already bitten tuinix
itself: `ESC[15~` used to be discarded three bytes at a time and leak its tail
as characters, and the only symptom a caller could see was unexplained
`Char` events.

**Protocol errors.** An application that understands a private sequence pair
(send a query, expect a reply) cannot tell "no reply yet" from "a reply arrived
and was thrown away by the decoder". Both are `None`.

## Guide-level explanation

An application that wants to know about discarded input opts in and logs it:

```rust
let mut decoder = tuinix::InputDecoder::new();
decoder.feed(&bytes);
while let Some(input) = decoder.next() {
    if let tuinix::Input::Unrecognized { bytes } = input {
        eprintln!("undecodable input: {:?}", bytes);
        continue;
    }
    handle(input);
}
```

An application that does not care is unaffected: the new variant is one more
arm to add to a `match` over [`Input`], and the behavior it replaces ("silently
consumed") is still what happens when the arm falls through to `_ => {}`.

## Reference-level explanation

Add a variant to `Input`:

```rust
pub enum Input {
    Key(KeyInput),
    Mouse(MouseInput),
    Unrecognized { bytes: Vec<u8> },
}
```

The variants are ordered `Key`, `Mouse`, `Unrecognized`, which is not a
meaningful order; the type derives `Ord` today, and that derive is already
questionable for `Key`/`Mouse` (see the note below).

The parser's `(Option<Input>, usize)` pairs change meaning: a returned `Some`
can now be a discard. Every site that returns `(None, n)` with `n > 0` becomes
`(Some(Input::Unrecognized { .. }), n)`, and `InputDecoder::next()`'s
"consumed but no input" loop disappears, because consumption always yields
something.

Sites that must be distinguished from discards:

- `(None, 0)` means "need more bytes" and stays `None`.
- `parse_sgr_mouse_sequence` returns `(None, 0)` while an SGR report is
  unterminated and `(None, end + 1)` for a malformed one; only the second is a
discard.
- The lone-`ESC` path is deliberately not a discard: `ESC` alone is held and
  then committed as the Escape key by `commit_escape()`.

The payload is the bytes that were dropped, which are exactly the bytes the
parser reported as consumed, so the caller can log or classify them. It is
owning (`Vec<u8>`) because the buffer is drained as soon as `next()` returns;
this makes `Input` non-`Copy`.

## Drawbacks

`Input` stops being `Copy`, which is a breaking change for every caller that
stores it in a `Copy` context. The crate is pre-1.0 and does not promise
stability, so the cost is a migration, not a compatibility breach.

It also enlarges the type's meaning: `Input` today is "what the user did", and
the new variant is "what the decoder could not understand about what arrived".
Those are different kinds of event, and a caller matching on `Input` to drive
application state now has an arm that represents no user action.

Discarded bytes are also attacker- or noise-controlled in the `Read`-driven
case (an application reading another program's ANSI output). Carrying them
verbatim makes it easy for a caller to log unbounded data unless it caps the
length itself.

## Rationale and alternatives

**Do nothing.** The strongest argument for the status quo is that the bytes are
already consumed, so this is pure observability, and observability that no
current caller has asked for. Against it: the decoder cannot be debugged from
its public API, and tuinix has already shipped two bugs whose only symptom was
indeciphable output from silently discarded input. "Defensible" is not the same
as "diagnosable".

**Keep `Input` and report discards out of band** (for example a counter or a
`take_unrecognized(&mut self) -> Option<Vec<u8>>` accessor, mirroring
`has_uncommitted_escape()`/`commit_escape()`). This keeps `Input: Copy` and
keeps the event type about user actions, at the cost of a second channel whose
contents the caller must remember to drain. It is the runner-up: it is the
right shape if the bytes are only wanted for logging, and the wrong shape if a
caller will ever dispatch on them.

**Return `Result<Input, Unrecognized>` from `next()`.** Makes discards
impossible to miss and forces every caller to handle them, including the
overwhelming majority that only want keys and mouse events. It also breaks
`Option`'s use as "nothing right now", which is the method's most common
result.

**Attach the bytes to the incomplete-sequence case** (make `None` carry a
reason, for example `next() -> NextInput` with `Pending`/`Decoded`/`Discarded`).
More explicit than an `Input` variant, but it changes the signature of `next()`
for every caller rather than adding a variant to a type they already match on,
and `None`-as-pending is the documented contract worth keeping.

**Expose raw bytes as `Char` events** (re-emit the dropped bytes as characters
so nothing is lost). This is exactly the bug fixed in
`20260915-bug-digit-dispatched-tilde-sequence-leaks-prefix.md`: re-reading a
known sequence's tail as characters is worse than dropping it, because it
invents input that the user did not produce.

## Unresolved questions

- Should the variant carry the whole run of discarded bytes, or the sequence
  type plus its parameters (`Csi`/`Ss3`/`Utf8` and the payload)? The bytes are
  what a caller can log; a classification is what a caller could dispatch on.
- Is `Unrecognized` the right name? Alternatives: `Undecodable`, `Discarded`,
  `Unknown`. The name should describe what the decoder did (it discarded the
  bytes) rather than what the bytes supposedly are, since the decoder is the
  party that gave up.
- Should the length be capped by the decoder (for example the first 16 bytes
  with a count), or is passing the bytes through and documenting "cap it
yourself" enough?
- `Input`, `KeyInput`, and `KeyCode` all derive `Ord` today. If a variant
  carrying `Vec<u8>` is added, that derive needs a story; dropping `Ord` from
  `Input` is a separate, smaller decision that could ride along or be settled
  independently.

## Future possibilities

- The same channel gives an application a hook for **unknown-but-plausible**
  sequences: a caller could recognize a discard it knows about and upgrade it to
  an action, which is how a binding layer would grow support for a terminal
  tuinix does not encode.
- A decoder that classifies rather than only reports could later expose the
  *reason* (`UnknownCsi`, `UnknownSs3`, `Malformed`, `InvalidUtf8`), which turns
  the variant into a debugging tool rather than a byte dump.
- If discard reporting proves useful, the analogous question for the *input*
  side arises: whether `InputDecoder` should also report bytes the application
  itself discarded with `discard_buffered_bytes()`.
