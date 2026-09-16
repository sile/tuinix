# Bug: A C1 control string never ends because `ST` is only recognized in its two-byte form

- Status: open

## Summary

A control string opened with a C1 introducer (`0x9d` OSC, `0x90` DCS, `0x9f` APC)
is only recognized as terminated by `ESC \`. The 8-bit form of `ST` (`0x9c`) is a
valid terminator for the same sequences, but the scanner does not look for it, so
a sequence that closes with `0x9c` never settles and the buffer holds it forever.

## Reproduction

The two-byte form of the same sequence settles; the one-byte form does not.

```text
input:   0x9d '0' ';' 'h' 'i' 0x9c        (C1 OSC terminated by 8-bit ST)
call:    InputDecoder::feed(&input); InputDecoder::next()
observed: None, and the buffer still holds all 6 bytes

expected for comparison:
input:   0x9d '0' ';' 'h' 'i' ESC '\'
result:  Some(Input::Unrecognized { bytes: <all 6> }) and the buffer is empty
```

The input may be fed in any split; the sequence is incomplete at every prefix,
so the split does not change the outcome.

## Observed behavior

`parse_control_string` waits on `find_control_string_end` (`src/input.rs`), which
matches only `BEL` (for OSC) and the two-byte `ESC \`:

```rust
0x07 if allow_bel => return Some(i + 1),
0x1b if bytes.get(i + 1) == Some(&b'\\') => return Some(i + 2),
```

`0x9c` is not matched, so the scanner runs off the end of the buffer and the
function returns `None`. `parse_control_string` then returns `(None, 0)`, which
`next()` treats as "more input will advance" and never consumes.

`0x9c` is currently classified elsewhere as an untranslated C1 byte that settles
one at a time (`test_parse_untranslated_c1_bytes_are_settled_one_at_a_time` in
`src/input.rs` asserts exactly that), but that path is never reached while a
control string is open — the body is opaque and is not scanned for C1 bytes.

## Expected behavior

`next()`'s rustdoc promises that it returns `None` only while no complete
`Input` can be produced from the bytes fed so far, and that a complete sequence
is eventually reported instead of being held. The bytes of a well-formed C1
control string are complete input, so holding them indefinitely breaks that
promise.

A C1 control string should end at either terminator its sequence defines:

- `0x9c` (`ST`, the 8-bit spelling)
- `ESC \` (`ST`, the two-byte spelling)

with `BEL` remaining valid for OSC alone. This is the same equivalence the C1
introducers follow since they were added: `0x9d` is read as `ESC ]`, `0x90` as
`ESC P`, `0x9f` as `ESC _`.

## Impact

A correctness problem reproducible from the public API. A terminal or program
that emits C1 control strings and closes them with `0x9c` — the spelling that
matches the introducer it just sent — never gets its sequence reported, and the
decoder's buffer grows without bound for the duration of the stream. This is the
same class as the two earlier "held forever" bugs (a complete sequence that
never settles): it costs memory and, since the sequence is never surfaced, the
bytes are lost.

## Notes

The two forms of `ST` are the only gap. `0x9d`/`0x90`/`0x9f` introducers and
both terminator spellings should be handled by one scanner; the body stays
opaque and single-byte C1 bytes inside it are not reclassified.
