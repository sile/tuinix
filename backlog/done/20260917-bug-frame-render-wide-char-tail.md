# Not a bug: an overwritten wide character does not leave its trailing cell stale

- Status: not-a-bug

## Summary

This report suspected that overwriting the leading cell of a wide character
leaves a stale trailing cell behind, which a per-cell renderer would draw as a
hole or as half of the old glyph. Investigation did not reproduce it: the frame
does not store the trailing cell at all, so there is nothing stale to leave
behind.

## Reproduction

Attempted and did not reproduce. The shape that was tried, in a 1-row frame of
at least 4 columns:

```text
push_char(wide('a'), width 2)   // occupies cols 0 and 1
push_char('b', width 1)         // overwrites col 0 only

// expected by the report: col 1 is stale
// measured: col 1 is blank as far as the frame is concerned
```

A probe covering this shape plus four nearby ones (narrow over wide, wide over
narrow, a write over the wide character's trailing column, and the same via
[`Frame::draw`](crate::Frame::draw)) all produced consistent output: the frame
never reported a cell belonging to a wide character that was no longer there.

## Observed behavior

A wide character is stored once, in its leading cell; the cells it covers beyond
that are not stored at all. `Frame::chars` yields it at its starting column and
skips the rest of its width, and the cell lookup returns `None` for a column
covered by a wide character that starts earlier. There is consequently no
stored trailing cell that a later write could leave behind.

The renderer keeps the invariant from the other side: it walks the frame and
emits only the positions `chars()` produces, so a column covered by a wide
character is never emitted as a cell of its own. Overwriting the leading cell
makes the wide character disappear from that iteration entirely, and the newly
written character is emitted at its own position.

## Expected behavior

Unchanged from the report: the frame in its own terms describes the whole row.
That holds.

## Impact

None. The suspicion came from writing a renderer that composes layers by
overwriting cells, which is the same overwrite case; the concern was legitimate
to check but the behavior was already correct.

Worth noting for the future: a wide character's cells are tied to the declared
width, so `Char` of width 2 is the case verified here. Nothing in the storage
prevents a caller from declaring other widths, and this report makes no claim
about those.

## Outcome

Closed as not-a-bug. The frame does not store a wide character's trailing cell,
so overwriting the leading cell cannot orphan one, and the renderer emits
exactly the positions `chars()` yields. `docs/frame-writes.md` now states this,
so the next reader does not have to derive it from the storage.

## Notes

Related to the RFC proposing random-access writes. That RFC would not change
this outcome, since a random-access write is subject to the same storage rule;
it does reopen the question of what a write over a wide character's cells leaves
behind, which is recorded in that RFC's open questions.
