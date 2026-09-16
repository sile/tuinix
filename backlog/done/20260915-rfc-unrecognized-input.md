# RFC: Report input the decoder chose to discard

- Status: accepted

## Summary

Add an `Input::Unrecognized { bytes: Vec<u8> }` variant that carries the bytes
`InputDecoder` consumed but could not decode, instead of swallowing them. The
bytes are already consumed today (the buffer advances, so nothing grows without
bound); what is missing is any signal that it happened, which makes a mis-decode
indistinguishable from silence.

Two smaller decisions ride along, because the variant's payload depends on them:
the two parser sites that report a fixed 3 bytes for a sequence that may be
longer learn to report its real end, and `Input` stops deriving `Ord`.

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

and the same shape appears in `parse_ss3_sequence` and `parse_simple_csi_key`,
and (as `(None, end + 1)`) in `parse_sgr_mouse_sequence`. Each of these consumes
bytes and returns no input. `parse_complex_csi_key` reaches the same result by
scanning for the terminator instead of trusting a fixed length, but the outcome
is identical: bytes consumed, nothing returned. `InputDecoder::next()` then
loops:

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
entirely, with no way to log what arrived.

Two shipped bugs had exactly this symptom. `ESC[15~` (F5 on an xterm-style
terminal) was discarded three bytes at a time and leaked its tail as
characters, so the caller saw unexplained `Char` events
(`done/20260915-bug-digit-dispatched-tilde-sequence-leaks-prefix.md`). `ESC[2J`
(clear screen) fell into a path that held the bytes as if the sequence were
still incomplete, so the buffer grew on every clear
(`done/20260915-bug-csi-parser-holds-incomplete-sequence.md`).

Both are fixed and both sequences now do the right thing, but the trap they
sprang out of is mostly unchanged: input the decoder gives up on is either
consumed silently or held as pending, and the caller cannot tell which from the
return value. (The one exception is `parse_csi_sequence`'s fixed 3-byte report,
which this RFC also fixes: as written it consumes 3 bytes of a sequence that may
be longer, so the tail leaks as characters, the same defect as the F5 bug.) A
decoder that reported the discard would have made the first bug visible on its
first occurrence instead of leaving a caller to infer it from
stray characters, and would have shown the second as a discard that never came
back rather than as a buffer that kept growing.

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
        // The decoder does not bound the payload; cap it where you log it.
        let shown = &bytes[..bytes.len().min(16)];
        eprintln!("undecodable input ({} bytes): {:?}", bytes.len(), shown);
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
meaningful order. `Input` derives `Ord` today; with a `Vec<u8>` payload that
derive would keep compiling (comparing byte vectors lexicographically) but would
be more misleading than before, so this RFC drops `Ord` and `PartialOrd` from
`Input` and leaves `KeyInput`/`KeyCode` alone. This mirrors
`done/20260915-rfc-size-ordering.md`, which removed the same derive from `Size`
for the same reason.

The parser's `(Option<Input>, usize)` pairs change meaning: a returned `Some`
can now be a discard. Every site that returns `(None, n)` with `n > 0` becomes
`(Some(Input::Unrecognized { .. }), n)`, and `InputDecoder::next()`'s
"consumed but no input" loop disappears, because consumption always yields
something.

The sites that return a discard today:

- `parse_csi_sequence` — an unknown byte after `ESC [`. This site currently
  reports a fixed 3 bytes regardless of how long the sequence actually is,
  which is both a truncated payload and a leak: `ESC [ 9 ~` reports 3 bytes, so
  `~` is left in the buffer and comes out as `Char('~')` on the next call. It
  changes to scan for the terminator with `find_parameter_terminator` (the
  helper `parse_complex_csi_key` already uses) and report `terminator + 1`.
- `parse_ss3_sequence` — an SS3 key outside `A B C D H F`. SS3 has no parameter
  run after `bytes[2]`, so a fixed 3 bytes is the real end here and this site
  keeps reporting 3.
- `parse_simple_csi_key` — a final byte outside `A B C D H F Z`.
- `parse_sgr_mouse_sequence` — an `ESC [ <` report that is terminated but does
  not parse as a button/coordinate triple; it reports `end + 1`.
- `parse_complex_csi_key` — a terminated parameter run whose terminator no
  branch claims (the `ESC [ 2 J` path); it reports `terminator + 1`.

The distinction those sites have to keep:

- `(None, 0)` means "need more bytes" and stays `None` everywhere.
- `parse_sgr_mouse_sequence` reports `(None, 0)` while the report is
  unterminated; only the malformed-but-terminated case is a discard.
- `parse_complex_csi_key` reports `(None, 0)` when the parameter run reaches the
  end of the buffer, because it might still grow into a known sequence; a
  discovered terminator is what makes it settle.
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
length itself. The decoder does not cap it: how much of a discard is worth
keeping is a trade-off the caller can see and the decoder cannot, the same
reasoning that kept a buffer bound out of `InputDecoder` in
`done/20260915-rfc-trim-buffered-bytes.md`. The Guide-level example shows the
caller capping what it logs.

## Rationale and alternatives

**Do nothing.** The strongest argument for the status quo is that the bytes are
already consumed, so this is pure observability, and observability that no
current caller has asked for. Against it: the decoder cannot be debugged from
its public API, and tuinix has already shipped two bugs in this exact corner
whose symptoms were indecipherable (stray characters from a leaked tail) and
invisible (a buffer that grew on every clear screen). "Defensible" is not the
same as "diagnosable".

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
so nothing is lost). This is the behavior that
`done/20260915-bug-digit-dispatched-tilde-sequence-leaks-prefix.md` had to be
fixed to stop: re-reading a known sequence's tail as characters is worse than
dropping it, because it invents input that the user did not produce.

## Unresolved questions

None. The five points this RFC had open are settled:

- **Payload: raw bytes, not a classification.** A classification would have to
  re-encode the interpretation the decoder just gave up on. Bytes can gain a
  classification later; a classification cannot recover bytes it dropped.
- **The two fixed-3-byte sites.** `parse_csi_sequence` learns to scan for the
  terminator, so its payload is the real sequence and `ESC [ 9 ~` stops leaking
  `~` as a character. `parse_ss3_sequence` reports 3 bytes because that is the
  real end of an SS3 sequence.
- **Name: `Unrecognized`.** `Discarded` would blur the line this RFC draws
  against `trim_buffered_bytes` (the decoder giving up vs. the application
  throwing bytes away), `Unknown` is an adjective with no noun, and
  `Undecodable` would invent vocabulary the crate does not otherwise use.
- **No decoder-side length cap.** Bounding the payload is a caller-visible
  trade-off, so the caller bounds it; the Guide-level example shows how.
- **Drop `Ord`/`PartialOrd` from `Input`.** Deriving them over a `Vec<u8>`
  payload compiles but is meaningless, and `Size` already set the precedent.
  `KeyInput` and `KeyCode` keep their derives.

## Outcome

Implemented in PR #35 (merge `394771a`) with the scope intact.

- `Input` gained `Unrecognized { bytes: Vec<u8> }` and lost `Copy`,
  `PartialOrd`, and `Ord`; `KeyInput` and `KeyCode` kept their derives.
- Every path that consumed bytes without producing an input now reports them:
  an unknown byte, an unknown SS3 sequence, an unknown `~` number, invalid
  UTF-8, an unparseable SGR mouse report, and an unknown CSI sequence.
- `parse_csi_sequence` scans for the terminator instead of returning 3, so
  `ESC [ 9 ~` is consumed whole rather than leaving its `~` behind.
  `parse_ss3_sequence` still reports 3, which is the real length.
- `InputDecoder::next()` lost its drain-and-retry loop: a non-zero consume
  always carries an event, so one `parse_input` call settles it.

One decision was made while implementing, on the SGR mouse prefix path. It used
to consume every byte it had read when it hit a byte that cannot appear in an
SGR report. Those bytes are now settled only for the `ESC [ <` marker, with the
rest left in the buffer, because bytes after the marker may be ordinary input
(`ESC [ < 1 2 a` is the marker, then `1`, `2`, and `a`). Consuming them with the
prefix would have invented the same kind of loss this RFC is about.

## Future possibilities

- The same channel gives an application a hook for **unknown-but-plausible**
  sequences: a caller could recognize a discard it knows about and upgrade it to
  an action, which is how a binding layer would grow support for a terminal
  tuinix does not encode.
- A decoder that classifies rather than only reports could later expose the
  *reason* (`UnknownCsi`, `UnknownSs3`, `Malformed`, `InvalidUtf8`), which turns
  the variant into a debugging tool rather than a byte dump.
- The two discards a caller can observe have different owners, and the docs
  should keep them apart: an `Unrecognized` event is the decoder giving up on
  bytes, while `trim_buffered_bytes()` is the application throwing them away
  to bound the buffer. That operation's own ergonomics were settled in
  `done/20260915-rfc-trim-buffered-bytes.md`.
