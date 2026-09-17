# RFC: Recognize bracketed paste as one input

- Status: accepted

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
        Input::Paste { bytes } => editor.insert_pasted(&bytes),
        Input::Key(key) => editor.handle_key(key),
        Input::Mouse(mouse) => editor.handle_mouse(mouse),
        Input::Unrecognized { .. } => {}
    }
}
```

The body arrives once as bytes, and the application decides whether to insert
it verbatim, re-indent it, decode it as text, or reject it. Without the variant,
the same paste is dozens of `Input::Key` events that the application would have
to buffer and reassemble itself, guessing where the paste ended.

## Reference-level explanation

The terminal sends `ESC [ 200 ~` before the text and `ESC [ 201 ~` after it. The
decoder has to hold the bytes between them until the closing marker arrives, and
then yield them as one value:

```rust
Input::Paste { bytes: Vec<u8> }
```

The variant sits alongside `Input::Unrecognized` and follows the same rule: it
reports bytes the decoder saw but did not interpret, unmodified.

This is the first input value that spans an arbitrary number of bytes, which
raises questions the current `Input` type does not answer:

- **Who owns the bounds?** The decoder holds the body until the terminator, so
  a paste of unbounded size is an unbounded buffer. The answer is the same as
  everywhere else in the decoder: the decoder holds no cap and no policy, and
  the application watches [`buffered_bytes()`](crate::InputDecoder::buffered_bytes)
  and reacts. A cap inside the decoder would be the kind of hidden trade-off
  tuinix avoids elsewhere, and a paste is not a special case of it.
- **Text or bytes?** The body is whatever the terminal put between the markers.
  It is usually UTF-8, but the decoder cannot know the application's encoding,
  and a paste can contain control bytes. `Vec<u8>` matches `Unrecognized` and
  leaves the interpretation to the caller; `String` is friendlier but has to do
  a lossy conversion that the decoder cannot justify.
- **What if the closing marker never arrives?** A truncated paste leaves the
  decoder holding bytes that will never be settled by more input. Nothing is
  lost and nothing is held forever behind the application's back: the held
  bytes are visible in `buffered_bytes()`, so the application can see the paste
  is not completing and act. As with any other input the decoder is still
  waiting on, the decoder does not decide when to give up.

## Drawbacks

- It is a new `Input` variant, so every exhaustive `match` on `Input` in a
  downstream application stops compiling. `Input` already grew a variant for
  `Unrecognized`, so this is an understood cost, but it is a second one.
- A held body is memory the decoder controls, which is a new kind of state for
  `InputDecoder`. Today the decoder never accumulates more than one incomplete
  sequence of a few bytes. The difference is that a paste is expected to be
  large, so the held bytes are not just a sequence prefix but potentially the
  bulk of what the application feeds.
- A paste whose closing marker never arrives holds its body until more input
  settles it. The decoder reports the growth through `buffered_bytes()` and
  leaves the response to the application, but an application that does not
  watch that count will simply accumulate.

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

**Why the body is bytes and not text.** The body is handed over as `Vec<u8>`,
unmodified, matching `Input::Unrecognized`. The decoder cannot know the
application's encoding, and a lossy `String` conversion would decide an
interpretation the decoder has no basis for. An application that wants text
converts it itself, which is where that decision belongs.

**Why the body is one value and not a stream.** A chunked variant would make
the application track "am I inside a paste" itself, which is part of what the
markers exist to tell it. Chunks can be added later without removing the
whole-paste variant, so starting with one value does not close that door.

**Why the decoder holds no cap.** A paste whose closing marker never arrives
is unbounded input, and the answer is the same as for any other input the
decoder is still waiting on: the decoder holds no cap and takes no policy. The
application watches `buffered_bytes()` and decides. Making a paste a special
case would mean the decoder hides a trade-off the application can see for
itself.

**Why nested markers are not handled.** The body ends at the first `ESC [ 201 ~`.
A terminal does not nest paste markers, so a body that contains the introducer
bytes is something the terminal is responsible for quoting; the decoder does
not guess. Documenting that is enough until a real terminal is found doing
otherwise.

## Unresolved questions

None. The settled decisions are recorded in the rationale above.

## Future possibilities

- An equivalent for the `ESC [ 201 ~` half arriving alone, which today would be
  an unknown marker and can be reported as `Unrecognized`.
- Enabling and disabling bracketed paste on the terminal as part of the
  terminal-mode API, so an application does not have to assume the terminal
  already does it.
- Chunked delivery, if the whole-body design proves too memory-hungry.
