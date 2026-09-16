# RFC: Recognize bracketed paste as one input

- Status: draft

## Summary

Bracketed paste lets a terminal tell an application "the bytes between these
two markers came from a paste, not from typing". tuinix does not recognize the
markers, so pasting text into an application built on tuinix delivers the text
as individual `Input::Key` events — indistinguishable from a user holding down
a key. This RFC proposes recognizing the markers and delivering the body as a
single input value.

## Motivation

`ESC [ 200 ~` opens a bracketed paste and `ESC [ 201 ~` closes it. A terminal
that supports it (xterm, and most terminals since) sends both markers around
the pasted text.

The decoder has no case for either number. Both are `~`-terminated, so
`parse_complex_csi_key` routes them to `parse_tilde_key`, which knows no key for
`200` or `201` and settles each marker as a small `Input::Unrecognized`. The
body between the markers has no such structure, so it is decoded normally: a
line of text becomes a run of `Input::Key(Char(..))` events, one per character,
with `Enter` for the newlines.

An application cannot tell that run apart from typing. Three things become
impossible or awkward:

- **Auto-indent and reformatting.** An editor that re-indents every newline is
  correct while typing and wrong while pasting, and nothing in the event stream
  marks the difference.
- **Paste detection.** "Select all" pasted over a buffer, or a pasted `q`, acts
  on the same events as the keystrokes it mimics.
- **Cost.** A large paste is one event per byte. A caller that redraws per event
  redraws thousands of times for one action, and the caller has no way to batch
  it without guessing at timing.

The markers exist precisely to solve this, and the decoder is the only place
that can see them before the body is split into keys.

## Guide-level explanation

An application reads a paste the way it reads any other input:

```rust
for input in decoder {
    match input {
        Input::Paste(text) => editor.insert_pasted(&text),
        Input::Key(key) => editor.handle_key(key),
        Input::Mouse(mouse) => editor.handle_mouse(mouse),
        Input::Unrecognized { .. } => {}
    }
}
```

The body arrives once, as text, and the application decides whether to insert it
verbatim, re-indent it, or reject it. Without the variant, the same paste is
dozens of `Input::Key` events that the application would have to buffer and
reassemble itself, guessing where the paste ended.

## Reference-level explanation

The terminal sends `ESC [ 200 ~` before the text and `ESC [ 201 ~` after it. The
decoder has to hold the bytes between them until the closing marker arrives, and
then yield them as one value.

This is the first input value that spans an arbitrary number of bytes, which
raises questions the current `Input` type does not answer:

- **Who owns the bounds?** The decoder holds the body until the terminator, so
  a paste of unbounded size is an unbounded buffer. The existing knob is
  `trim_buffered_bytes()`, which discards from the front and so would corrupt
  a held paste rather than bound it. A cap inside the decoder would be the kind
  of hidden trade-off tuinix avoids elsewhere.
- **Text or bytes?** The body is whatever the terminal put between the markers.
  It is usually UTF-8, but the decoder cannot know the application's encoding,
  and a paste can contain control bytes. `Vec<u8>` matches `Unrecognized` and
  leaves the interpretation to the caller; `String` is friendlier but has to do
  a lossy conversion that the decoder cannot justify.
- **What if the closing marker never arrives?** A truncated paste leaves the
  decoder holding bytes that will never be settled by more input. This is the
  same permanent-hold shape as the CSI bugs already fixed, and it needs a
  defined answer (a terminator scan, a size cap, or an explicit way for the
  application to abandon the held bytes).

## Drawbacks

- It is a new `Input` variant, so every exhaustive `match` on `Input` in a
  downstream application stops compiling. `Input` already grew a variant for
  `Unrecognized`, so this is an understood cost, but it is a second one.
- A held body is memory the decoder controls, which is a new kind of state for
  `InputDecoder`. Today the decoder never accumulates more than one incomplete
  sequence of a few bytes.
- The nested-marker question is real: a paste can contain the introducer bytes
  in its text, and terminals differ on whether they escape them. The decoder
  needs a defined behavior either way.

## Rationale and alternatives

**Why not deliver the body as it arrives, in chunks.** A streaming variant
(`PasteChunk`) keeps memory bounded and lets a caller render progressively. The
cost is that the caller must track "am I inside a paste" itself, and a chunk is
not an input value so much as a state change. If the memory bound turns out to
be the hard part of this RFC, chunks are the escape hatch, and they can be added
later without removing the whole-paste variant.

**Why not let the application reassemble the keys.** The markers are visible to
the application as `Unrecognized` today, so an application could in principle
detect the opening marker, buffer subsequent keys, and stop at the closing one.
This pushes terminal knowledge into every application, and the closing marker
arrives in the same stream as the text, so the reassembly has to re-implement
part of the decoder. If enough applications do that, the decoder should do it.

**Why not require an explicit enable.** Some terminals only send the markers
when the application asks with `ESC [ ? 2004 h`. tuinix has no notion of
writing escape sequences to the terminal, so a decoder-side `Input` variant
cannot assume the markers will appear. This RFC does not depend on enabling:
if the markers arrive, they are recognized; if they do not, nothing changes.
Whether tuinix should also offer to enable bracketed paste on the terminal is a
separate question (see below).

## Unresolved questions

- Body type: `Vec<u8>` or `String`.
- Whether the body is one value or a stream of chunks; this decides the memory
  story and whether `trim_buffered_bytes()` has any role while a paste is held.
- The bound on a held paste that is never closed, and what the application can
  do about it.
- Whether the decoder handles nested markers and escaped introducers, or
  documents that the terminal guarantees they do not occur.

## Future possibilities

- An equivalent for the `ESC [ 201 ~` half arriving alone, which today would be
  an unknown marker and can be reported as `Unrecognized`.
- Enabling and disabling bracketed paste on the terminal as part of the
  terminal-mode API, so an application does not have to assume the terminal
  already does it.
- Chunked delivery, if the whole-body design proves too memory-hungry.
