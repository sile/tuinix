# RFC: Add random-access character writes to `Frame` (superseded)

- Status: accepted

## Summary

Add a method that stores a styled character at an explicit
[`Position`](crate::Position) — overwriting whatever is there — independent of
the frame's append-only cursor, for example
`Frame::put_char(&mut self, at: Position, ch: Char)`.

## Motivation

`Frame` exposes only a forward cursor: [`push_char`](crate::Frame::push_char)
and [`push_newline`](crate::Frame::push_newline). That is a good fit for a
renderer that walks the screen in row-major order once and emits as it goes.

It is a poor fit for the way a real application composes a screen, which is by
*layers*. A terminal multiplexer builds one frame out of independent pieces:
the tiled panes first, then free-floating overlays on top, then a software
keyboard across the bottom. Later layers overwrite earlier ones at the same
cells (an overlay covering a pane, the keyboard covering pane content) and are
otherwise sparse — a keyboard occupies the bottom few rows and nothing else.

With a forward-only cursor, painting a sparse later layer means the caller has
to append a run of blanks to walk the cursor from wherever it is to the next
cell it actually wants to write. A real caller ends up with a helper shaped
like this:

```rust
// advance the cursor to `at`, filling the gap with blanks, then write
fn put_at(frame: &mut Frame, at: Position, ch: Char) {
    while (frame.cursor().row, frame.cursor().col) < (at.row, at.col) {
        if frame.cursor().col >= frame.size().cols {
            frame.push_newline();
        } else {
            let _ = frame.push_char(' ');
        }
    }
    let _ = frame.push_char(ch);
}
```

That helper is pure cursor bookkeeping. It writes cells it will immediately
overwrite (blanks under a keyboard row that a later write fills), and it has to
reproduce the frame's own wrap rules to advance correctly. Every layered
renderer writes it, and every one of them has to agree with the frame's wrap
logic without the frame telling it anything.

A random-access write removes the helper entirely: the caller names the cell it
wants to set.

## Guide-level explanation

Before, writing one cell of a later layer required walking there:

```rust
// walk from the current cursor to (row, col), then write
while frame.cursor().row < row {
    frame.push_newline();
}
while frame.cursor().col < col {
    let _ = frame.push_char(' ');
}
let _ = frame.push_char(ch);
```

After:

```rust
frame.put_char(Position { row, col }, ch);
```

`push_char` and `push_newline` remain; a renderer that walks the screen once
still uses them. `put_char` is for the caller that knows the cell it wants.

## Reference-level explanation

```rust
impl Frame {
    /// Store `ch` at `at`, overwriting the cell there and any cell `ch`
    /// displaces by its width.
    ///
    /// The cursor is not moved. `at` is clipped against the frame size: a
    /// position outside the frame is ignored, and a character whose width runs
    /// past the last column is written up to the edge (matching what
    /// `push_char` does at a row boundary).
    pub fn put_char(&mut self, at: Position, ch: Char) { /* ... */ }
}
```

Design points to settle:

- **Cursor interaction.** `put_char` does not move the cursor, so a caller can
  mix random-access writes with a row-major walk without the two fighting.
  Whether that is the right default, or whether `put_char` should also move the
  cursor, is an open question.
- **Wide characters.** `Char` carries a width. A write at a position must
  decide the same thing `push_char` decides at a row edge: does the wide
  character get clipped, and what happens to the cell it could not fill? The
  answer must be identical in both paths, which is an argument for routing
  `put_char` through the same storage step as `push_char` rather than a second
  implementation.
- **Overwrite semantics.** `put_char` overwrites; it does not merge styles or
  track what it displaced. If a later layer writes a narrow character over the
  left half of an earlier wide character, the earlier character's right half is
  the caller's problem to repaint. Documenting this is enough for now; making the
  frame repair it is a larger question (see the related wide-tail bug report).



## Alternatives

### Keep the caller-side `put_at` helper

Rejected as the resting place. It is correct only if it mirrors the frame's
wrap rules exactly, which the frame does not expose, so correctness is
maintained by hand in every consumer.

### Build the frame from a cell map and paint once

This is what a real caller can do today, and it works: collect
`(Position, Char)` into a map, then walk it in row-major order with the cursor
API. It is also more allocation and more moving parts than the frame storing
cells where the caller says. If the project would rather keep `Frame`
append-only as a deliberate constraint, this alternative is the status quo and
this RFC should be rejected — but then the append-only constraint should be
stated as intentional, because it is currently discoverable only by using it.

### A builder type that collects cells and materializes a `Frame`

Rejected for the same reason: it moves the cell map out of `Frame` into one
more type the caller has to learn.

This proposal composes with the append-only cursor rather than replacing it: a
renderer walks a whole screen with `push_char`/`push_newline`, and fills a
sparse later layer with `put_char`. Neither has to re-derive the other's
wrapping. The write model the cursor follows — bounds, clipping, and what the
write position means — is described in `docs/frame-writes.md`; that document
covers only `push_*`, and its closing paragraph names the open question this
proposal has to answer.

## Drawbacks

- Two ways to write into a frame (`push_char`, `put_char`) is one more than
  zero-overlap ideal. They serve genuinely different walks, so the overlap is
  small, but it is not none.
- Random access makes it easy to write a cell and then paint over it in the
  same frame, which is wasted work the append-only API made awkward. That is a
  caller-level cost, not a correctness one.

## Open questions

- Does `put_char` move the cursor or leave it? (Sketch assumes leave it.)
  This is the deciding question for the whole proposal, and it is what
  `docs/frame-writes.md` stops short of: as long as the write position only
  moves forward through `push_*`, a caller can predict a clip from
  `next_position()` and `size()`; a position-addressed write that also moves the
  write position would break that rule for every `push_*` that follows it, so
  whichever way this is settled has to be written into that document as well.
- Is a scalar `put_char` enough, or is a `put_text(at, text, style)` that lays
  a run down from a position worth having, given labels in a keyboard row are
  exactly that?

## Superseded

Superseded by
[`20260917-rfc-frame-write-model.md`](../20260917-rfc-frame-write-model.md),
which reached this proposal's core question — whether a position-addressed write
leaves the cursor alone — by deciding that the cursor should not exist at all.
The question this RFC left open ("does `put_char` move the cursor?") cannot be
answered the way it is framed here, because moving or not moving the cursor is
the thing that keeps two coordinate systems alive. The follow-up RFC drops
`push_char`/`push_newline`/`push_tab`/`next_position` and the `tail` field, so
there is nothing for a write to move, and `put_char` returns the next position to
the caller instead.

The two things this RFC decided and the follow-up keeps: a position-addressed
write leaves no cursor behind, and a clip is a normal outcome rather than a
failure to return. Its open question about `put_text` is carried forward as a
future possibility.
