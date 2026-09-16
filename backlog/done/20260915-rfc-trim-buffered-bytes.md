# RFC: Trim the decoder buffer to a caller-chosen length

- Status: accepted

## Summary

Change `InputDecoder::discard_buffered_bytes(len)` into
`trim_buffered_bytes(max_len)`, so a caller enforcing its own buffer bound
passes the length to *keep* rather than the length to throw away. The decoder
keeps no bound of its own; the caller still owns the policy, but writing it no
longer requires reading `buffered_bytes()` and subtracting.

## Motivation

`InputDecoder` does not bound how many bytes it holds. That is deliberate: how
much unparsed input is worth waiting for is a trade-off only the application can
judge, so the decoder exposes the buffer length and an operation to shorten
it, leaving the threshold to the caller.

That division of responsibility is right, but the shape of the operation makes
the caller do arithmetic to express it. Enforcing a bound of 4096 bytes looks
like this:

```rust
if input.buffered_bytes() > MAX_BUFFERED_BYTES {
    input.discard_buffered_bytes(input.buffered_bytes() - MAX_BUFFERED_BYTES);
}
```

The threshold the caller cares about (`MAX_BUFFERED_BYTES`) is not the value
passed to the method. The argument is "how many bytes to drop", which is a
derived quantity: the caller reads the current length, subtracts the length it
wants to keep, and passes the difference. A caller writing this has to hold two
quantities in mind (the bound and the excess) when it only has one (the bound).

The method is also named after the wrong noun. `discard_buffered_bytes` says
what happens to the bytes on the way out; the caller's intent is "do not let
the buffer exceed this".

This is a papercut, not a defect: the current call is correct and the "drop the
excess" reading is documented. But the buffer bound is the one place where an
application interacts with the decoder's holding behavior, and it is the
interaction most likely to be copied from the example in `examples/demo.rs`.
Making the argument the bound itself removes the subtraction from every such
caller.

## Guide-level explanation

An application that wants to bound how much unparsed input the decoder holds
passes the bound it wants:

```rust
const MAX_BUFFERED_BYTES: usize = 4096;

// Keep at most `MAX_BUFFERED_BYTES` bytes of unparsed input.
input.trim_buffered_bytes(MAX_BUFFERED_BYTES);
```

The call is unconditional: when the buffer is already at or below the bound it
does nothing, so it needs no `if` around it. The application still decides
*when* to trim (after each read, or only when `buffered_bytes()` grows past some
larger mark) and still decides the bound.

`buffered_bytes()` remains the query for a caller that wants to inspect the
buffer — to log it, to decide whether trimming is needed at all, or to warn that
input is arriving that the decoder cannot parse.

## Reference-level explanation

Replace the method in `src/input.rs`:

```rust
impl InputDecoder {
    /// Shortens the buffer to at most `max_len` bytes, discarding from the
    /// front, and returns how many bytes were discarded.
    ///
    /// This is how an application enforces its own bound on
    /// [`buffered_bytes()`](Self::buffered_bytes): the decoder never drops bytes
    /// on its own, because only the application knows whether discarding a
    /// partial sequence is acceptable. A call with `max_len` greater than or
    /// equal to the current length discards nothing and returns `0`.
    pub fn trim_buffered_bytes(&mut self, max_len: usize) -> usize {
        let excess = self.buf.len().saturating_sub(max_len);
        self.buf.drain(..excess);
        excess
    }
}
```

Differences from the current method:

- The argument is a target length, not a count to remove. `trim_buffered_bytes(0)`
  empties the buffer, which is the `discard_buffered_bytes(usize::MAX)` case
  today spelled with the value the caller actually has.
- The return value keeps the old meaning (bytes discarded), so a caller that
  logs the loss still can. It is now always `saturating_sub`-derived, so the
  `len.min(self.buf.len())` clipping in the current implementation becomes
  implicit.
- The receiver is `&mut self` for the same reason as before: the operation
  mutates the buffer.

The doc must state the two facts that survive the rename: the decoder never
discards on its own, and trimming can cut through the middle of an incomplete
sequence, because the operation is byte-oriented while the parser is
sequence-oriented. A caller that wants to trim only at sequence boundaries can
check `buffered_bytes()` first, but the decoder does not offer a
sequence-boundary trim: after a very large incomplete sequence (for example a
paste that contained a stray `ESC` followed by megabytes of text), the whole
run is one pending sequence and trimming is the only way to bound it.

Call sites to update:

- `examples/demo.rs` — the bound check at the read loop; the `if` and the
  subtraction both go away.
- `src/input.rs` — the `InputDecoder` struct doc, the `feed()` doc, and the
  `buffered_bytes()` doc each point at the method by name.
- `src/input.rs` tests — `test_input_decoder_buffered_bytes_and_discard`
  asserts on the old argument convention (`len`, `usize::MAX`) and on the
  over-large case; it becomes a test of the trim-to-bound convention.

## Drawbacks

It is a breaking change to a public method for a naming and ergonomics
difference, not for a behavior difference, so every caller has to be touched for
a change no caller asked for. Migration is mechanical — pass the bound instead
of the excess — but a caller that reads the new name as "discard at most
`max_len` bytes" would be wrong, which is why the doc has to say the argument is
a length to keep.

It also leaves `discard` vocabulary in the crate's history for a method whose
current name is referenced in doc links across `src/input.rs`; those links move,
which makes the change slightly wider than the one method.

## Rationale and alternatives

**Do nothing.** The current call is correct and documented, and the subtraction
is two operations. Against it: the bound is the value the caller has, and an API
whose argument is "the amount by which you exceeded the bound you are not
passing" asks every caller to reproduce the same subtraction. `examples/demo.rs`
and `src/input.rs`'s own test already reproduce it, which is the signal that the
shape is awkward rather than rare.

**Make the decoder hold the bound** (a `with_max_buffered_bytes(n)` constructor,
with the decoder discarding on its own once the buffer passes it). This makes the
caller's code shortest, and it is rejected: the bound is a policy with a
user-visible trade-off (a real sequence can be cut in half by it), and a
foundational layer must not hide such a decision. A caller could not see which
threshold applied or override it per input source. tuinix's own convention is to
expose mechanism and leave policy to the caller, and a buffer cap hidden in the
decoder is exactly the case that convention exists to prevent.

**Make the argument optional** (`discard_buffered_bytes()` with no argument,
emptying the buffer). Smaller change, and "drop everything" covers the bound use
case (`if buffered_bytes() > MAX { discard_buffered_bytes(); }`). It is rejected
because it turns the bound into an all-or-nothing decision: a caller that wants
to keep the most recent 4096 bytes of a 1 MiB paste keeps nothing instead, and
the method can no longer express "keep this much".

**Rename without changing the argument** (`discard_buffered_bytes` →
`truncate_buffered_bytes`, or keep the name and only improve the doc). This is
the smallest diff and it leaves the arithmetic in place; it is rejected because
the arithmetic is the part that is awkward, and a doc cannot remove it from a
call site.

**Split the query from the operation differently** (a
`take_buffered_bytes(max_len) -> Vec<u8>` that removes and returns the bytes, so
nothing is silently lost). This makes the discard observable, which is a real
virtue, but it hands the caller a `Vec<u8>` allocation on a path whose whole
purpose is to shed memory under load, and it overlaps with the separate proposal
to report input the decoder itself discards. Keeping trimming in place, losing
the bytes, is the cheaper operation for the caller that wants a bound.

## Outcome

Implemented in [#34](https://github.com/sile/tuinix/pull/34) (merged as
`3ce54ca`). `InputDecoder::discard_buffered_bytes(len)` is now
`trim_buffered_bytes(max_len)`, returning the number of bytes discarded;
`buffered_bytes()` stays as the query. `examples/demo.rs` now calls
`trim_buffered_bytes(MAX_BUFFERED_BYTES)` unconditionally, without reading the
length or subtracting. Scope unchanged from the Decision section.

## Unresolved questions

None.

The name was settled as `trim_buffered_bytes`. It is the only candidate that does
not fight the direction of the cut: `truncate_buffered_bytes` borrows
`Vec::truncate`'s wording for "shorten to a length", but that wording carries
"cut the end", while this cuts the front. `cap_buffered_bytes` reads as a stored
limit rather than an operation, and `keep_buffered_bytes` describes the argument
but not the action.

The return value stays `usize`. No caller uses it today, but the count falls out
of the `saturating_sub` anyway, and it is what a caller would want for logging
how much input it shed.

`buffered_bytes()` stays as the query. `trim_buffered_bytes` does not report the
length it started from, so a caller that wants to notice an oversized buffer — or
to trim only past some mark rather than on every read — needs it. The in-crate
tests use it the same way.

## Future possibilities

- A trimming operation that cuts at a sequence boundary
  (`trim_buffered_bytes_to_sequence()`) would let a caller shed the front of a
  long incomplete run without risking a half-cut sequence. It is a different
  operation from this one, and it is not proposed now because the byte-oriented
  cut is what an application under memory pressure actually needs.
- If the decoder later reports the input it discards itself (see
  `20260915-rfc-unrecognized-input.md`), the two discards have different
  owners: `trim_buffered_bytes` is the application throwing bytes away, and an
  `Unrecognized` event is the decoder giving up on them. The vocabulary in the
  docs should keep those apart.
