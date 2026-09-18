# RFC: Address frame writes by position instead of by a cursor

- Status: accepted

## Summary

Drop the write cursor from [`Frame`](crate::Frame) and require every write to
name the position it targets. The forward-cursor methods
(`push_char`/`push_newline`/`push_tab`/`next_position`) and the `tail` field go
away; in their place come `put_char(at, ch)` and a renamed `draw`
(`put_frame(at, &frame)`), plus a predicate that reports whether a character
would fit at a position.

## Motivation

A frame currently has two ways to say where a character goes, and they do not
know about each other.

- `push_char`, `push_newline` and `push_tab` advance an internal cursor
  (`tail`), which `next_position()` exposes as the position the next `push_*`
  will write.
- `draw` takes an explicit `Position` and ignores the cursor entirely.

So a frame holds two coordinate systems at once. That is fine while only one is
in play, but it leaves "where does the next write go?" unanswered as soon as a
caller mixes the two: after `draw` lands a sparse layer at an explicit position,
a following `push_char` still writes wherever `tail` was left, which is rarely
where the caller thinks.

Composing a screen out of layers is the case that exposes this. A terminal
multiplexer paints the tiled panes first, then free-floating overlays on top,
then a software keyboard across the bottom. Later layers overwrite earlier ones
at the same cells and are otherwise sparse. With only a forward cursor, writing
one cell of a later layer means walking there with `push_char(' ')` calls that
the next write immediately paints over, and reproducing the frame's wrap rules
by hand to walk correctly. The frame knows the wrap rules and does not tell the
caller.

Naming a position for every write removes the second coordinate system and the
helper that reconciles them. There is then one rule — a write goes where it
says — rather than two that have to be kept in agreement.

## Guide-level explanation

Before, a caller that knew the cell it wanted had to reach it through the
cursor:

```rust
// walk to (row, col) with blanks, then write
while frame.next_position().row < row {
    frame.push_newline();
}
while frame.next_position().col < col {
    let _ = frame.push_char(' ');
}
let _ = frame.push_char(ch);
```

After, the position is an argument:

```rust
frame.put_char(Position { row, col }, ch);
```

A renderer that fills the screen in row-major order writes the same loop it
always did, just passing the position along instead of relying on hidden state:

```rust
let mut at = Position::ORIGIN;
for ch in text {
    if !frame.fits(at, ch) {
        at = at.next_line();
        if !frame.fits(at, ch) {
            break; // past the bottom row, or too wide for an empty row
        }
    }
    at = frame.put_char(at, ch);
}
```

The wrap test asks `fits` twice around the line break: once where the character
was, and once at the start of the next row. Both the right edge and the bottom
edge are answered by the same predicate, so the loop never consults `size()`
directly and never needs a separate "is there another row?" test. The second
check also ends the loop on a frame too narrow to hold the character at all,
rather than advancing forever.

`put_char` returns the position just past the character it wrote, so the caller
carries the position instead of asking the frame for it. Tab stops and line
breaks are position arithmetic too:

```rust
at = at.next_tab_stop(tab_width);   // advance to the next tab stop
at = at.next_line();                // move to column 0 of the next row
```

## Reference-level explanation

```rust
impl Position {
    /// The position at column 0 of the next row.
    pub const fn next_line(self) -> Self;

    /// The next tab stop at or after this position, stepping by `tab_width` columns.
    pub const fn next_tab_stop(self, tab_width: usize) -> Self;

    /// The position this far to the right.
    pub const fn advance(self, width: usize) -> Self;
}

impl Frame {
    /// Whether a character written at `at` would be drawn.
    pub fn fits(&self, at: Position, ch: Char) -> bool;

    /// Draw `ch` at `at` and return the position just past it.
    pub fn put_char(&mut self, at: Position, ch: Char) -> Position;

    /// Draw every cell of `source` with its top-left corner at `at`.
    pub fn put_frame(&mut self, at: Position, source: &Frame);
}
```

Removed: `Frame::tail`, `Frame::next_position`, `Frame::push_char`,
`Frame::push_newline`, `Frame::push_tab`. `Frame::draw` is renamed to
`put_frame`, with no change in behavior.

### One coordinate system

Every write names a `Position`. There is no cursor to read or to leave behind,
so the `tail` field is deleted rather than kept as a convenience. A caller that
wants to sweep the screen keeps the position in a local and threads it through
`put_char`'s return value.

This also removes the `push_*` idiom where a clip is predictable from
`next_position()` and `size()` ahead of the write. The same prediction is still
available, but as `fits(at, ch)` — which asks the question the caller actually
has ("will this be drawn here?") instead of asking where the hidden cursor is.

### Clipping is a delivery detail, not an error

`put_char` writes when `fits(at, ch)` is true and draws nothing when it is
false. Either way it returns `at.advance(ch.width())`, the position just past
the character, so a run of writes advances identically whether or not each cell
landed. The return type is `Position`, not `Option` or `Result`, because a
character that does not fit is a normal outcome for a caller sweeping a row,
not a failure to report.

`fits` combines the two bounds `put_char` checks and matches what current
`push_char` and `draw` both do today:

```rust
at.row < self.size().rows && at.col + ch.width() <= self.size().cols
```

The character is tested as the range of cells it would occupy, not as a point:
for a width-2 character at the last column, `at.col + 2 <= cols` fails and
nothing is drawn. A character that does not fit the remaining columns of a row
is never split across the row edge — none of its cells land, and the caller
moves on (`next_line`) or stops.

Splitting this into `contains` (position in the frame) and a separate width
check would make every caller AND the two together anyway. It is named `fits`
rather than `is_writable` because the predicate reports what will be *drawn*,
not what is *allowed*: an out-of-range write is a valid call that happens to
produce nothing.

### Overwriting follows `draw`

A write at a position replaces what is there. When the target cells overlap a
wide character that starts to the left, that wide character is removed whole
first, then the cells the new character covers are cleared, then the new
character is stored. This is exactly what `draw` does today, so the storage step
is shared: a private `put_cell(at, ch)` holds the overwrite rule and both
`put_char` and `put_frame` call it. `put_frame` does not loop over `put_char`;
the two differ in what they do at the edges (see below) and `put_frame` has no
use for a returned position.

### `put_frame` returns nothing

`put_frame` returns `()` where `put_char` returns a `Position`. A frame is many
cells, so "the position just past the character" has no single answer, and a
caller placing a rectangle is not sweeping a run and has no next write to aim.
The asymmetry with `put_char` is deliberate: the returned position exists to
carry a sweep, and `put_frame` does not sweep.

### `put_frame` keeps `draw`'s behavior

`put_frame` is the former `draw`. Its rule: for each cell of the source, a cell
that fits is written over the target, a cell that does not fit is skipped, and
a cell the source never wrote is written as `Char::BLANK` — so `put_frame`
replaces a rectangle, it does not merge one. That last rule is why it stays a
method of its own rather than becoming a loop over `put_char`, and why the name
is `put_frame` (the `put_` prefix groups it with `put_char`) with the
blank-fills-the-rectangle behavior documented on the method rather than encoded
in the name.

## Drawbacks

- Every caller that builds a screen by sweeping it now threads a `Position`
  through the loop, where before the frame carried it. That is more to write for
  the simple row-major renderer — the price of removing hidden state.
- Deleting `push_char`/`push_newline`/`push_tab` is a breaking change with no
  compatibility shim. Every consumer updates at once.
- `put_char` returning the next position is only useful to callers sweeping a
  row; a caller writing scattered cells discards it. Making it `()` would cost
  the sweep its natural loop, so the return stays.

## Rationale and alternatives

### Why not keep the cursor and just add `put_char`?

This was the original shape of the proposal: keep `tail`, add a
position-addressed `put_char` that does not move it, and let the two coexist.
That is the smallest change and it composes — `draw` already ignores the cursor,
so a non-moving `put_char` adds nothing new. But it leaves the motivating
problem in place: two coordinate systems still exist, and "where does the next
write go?" is still a question a caller has to hold in mind. It also has to be
written into `docs/frame-writes.md` as a rule, which is a sign the model is
carrying more than it needs to. If the two systems are going to be reconciled
in documentation anyway, removing one is the smaller long-run surface.

### Why not `seek` plus the existing `push_*`?

A `seek(Position)` that stored the cursor, composed with `push_char`, would let
a caller place the cursor and then write — so `put_char(at, ch)` reduces to
`seek(at); push_char(ch)`, and the cursor API could stay.

This works for one character and stops working at the row edge. `push_char`
clips a character that runs past the right-hand column instead of wrapping, so a
caller laying down a run from an arbitrary position cannot express "write what
fits, then continue on the next row" as a `seek`-plus-`push` sequence without
recomputing, per character, where the row ends. `put_frame` needs the same
row-wise clip and cannot be built from `seek` and `push_char` at all. Since the
positional write is needed regardless, `seek` would be a third way to say where
a write lands, not a replacement for one.

### Alternatives considered and rejected

- **Keep `push_*` as the only API and reject this.** The status quo is
  defensible: `draw` already reaches arbitrary positions, and a caller can
  collect cells in a map and paint them once. Rejected because it makes the
  caller re-derive the frame's bounds and clip rules, and because it leaves the
  two-coordinate-system problem undocumented as if it were intended.
- **Build frames from a separate cell-map type.** Rejected: it moves storage out
  of `Frame` into one more type to learn, when `Frame` already is the cell map.
- **A `put_text(at, text, style)` run-writer.** Out of scope here. A run is a
  loop over `put_char` with a width test, which a caller can write; a dedicated
  method can be proposed later if it earns its place.
- **Add `clear`/`erase` methods for deleting cells.** Rejected for now: writing
  `Char::BLANK` at a position already removes the cell that was there, so a
  separate delete call adds surface without adding a capability.

## Unresolved questions

None. The one adjacent question — whether the tab width should be a non-zero
type — is tracked in `20260917-rfc-nonzero-widths.md`: it does not stand alone
because `Char::width()` makes the same "1 or more" promise, so changing one
without the other would split that promise across two representations.

## Future possibilities

- `put_text(at, text, style)` — a run-writer with a wrap policy — becomes easy
  once `put_char` returns the next position.
- A `put_frame` that merges instead of replacing its rectangle (leaving source
  blanks transparent) would let overlays compose without a blank rectangle; the
  current behavior is the one `draw` has today.
- Deleting the cursor makes `Frame` a pure cell map, which makes it
  straightforward to snapshot, compare, or serialize a frame later.
