---
Created: 2026-09-17
Status: open
---

# Bug: An overwritten wide character may leave its trailing cell stale

## Summary

When a wide character (width 2) is written and a later character is written
over only its leading cell, the trailing cell it occupied is not necessarily
cleared or rewritten. If the frame's rendered output is computed per cell, the
stale trailing half can be drawn as a hole or as the old character's second
column. The exact trigger is not yet pinned down — this report records the
suspicion and the smallest inputs to try, and should be converted to an RFC if
investigation shows the current behavior is in fact defended by the documented
contract.

## Reproduction

Not yet minimized. The shape to try, in a 1-row frame of at least 4 columns:

```text
push_char(wide('a'), width 2)   // occupies cols 0 and 1
push_newline()                  // or seek back to col 0
push_char('b', width 1)         // overwrites col 0 only

// observed: ? at col 1
// expected: col 1 is either cleared or explicitly part of what 'b' wrote
```

Any framing of the test must pin down how the second write reaches the cell the
first one's tail occupies — cursor movement, `push_newline` on a fresh row, or a
random-access write if one is added — because the behavior may differ between
them.

## Observed behavior

The frame stores cells keyed by position. A wide character stores its glyph in
its leading cell; whether its trailing cell is marked as "consumed by the
previous cell" or holds the continuation is the crux, and the current storage
and its rendering do not make that explicit in the rustdoc.

The suspicion is specifically about the renderer and the frame's diff/prev
frame, not about the stored cells: `Frame::render` walks the frame and skips a
cell whose stored value equals the previous frame's, which is what makes a
re-render cheap. If the trailing cell is a blank that was *already* blank in the
previous frame, it may be skipped, even though the leading cell changed from a
wide character to a narrow one and the terminal's own cursor moved past the
trailing column — leaving whatever the terminal had there.

## Expected behavior

After any write, the cells the frame reports must describe the whole row, so
that rendering the frame produces exactly the intended screen regardless of what
was there before. In particular, overwriting the leading cell of a wide
character must not leave the frame claiming a wide character is drawn where it
is not.

## Impact

Not yet observed in a running program; found while writing a renderer that
composes layers and overwrites one layer's cells with another's, which is
exactly the overwrite case above. If real, it is a correctness problem for any
caller that draws wide characters and later paints over them — the visible
symptom is a duplicated or lingering half-glyph.

Correctness, not ergonomics, so it belongs as a bug once reproduced; if
reproduction shows the behavior is the documented intent (for example because
the frame explicitly requires the caller to erase before overwriting), this
should move to `done/` as not-a-bug with that reason, or become an RFC for a
`put_char` that repairs its own overlap.

## Notes

Related to the RFC proposing random-access writes: if that RFC is implemented,
the overwrite case becomes routine and this bug (if real) becomes routine too.
The two should be settled with that in mind — either `put_char` is responsible
for clearing what it displaces, or it is documented as not responsible and this
bug is resolved as intended.
