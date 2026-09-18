---
Created: 2026-09-17
Status: draft
---

# RFC: Tell the caller *why* `push_char` clipped

## Summary

Give `Frame::push_char` a return value that distinguishes "the character did
not fit because it would wrap off the bottom" from "the character did not fit
because it would overflow the right edge", or add a `try_push_char` that
returns that distinction while `push_char` keeps its current `bool`.

## Motivation

`push_char` returns `false` both when the cursor has run past the last row and
when the character would not fit in the remaining columns of the current row.
Its rustdoc already tells the caller to work out which one happened from the
cursor position and the frame size:

> Whether the clip is a horizontal one (the character did not fit in the
> remaining columns) or a vertical one (the cursor was already past the last
> row) is decided by the caller from the position and size.

This is the position a layer-composition renderer is in constantly. A renderer
writes several layers into the same frame (panes, then overlays, then a
software keyboard) and wants to keep painting the current layer until it is
done, while stopping cleanly at a row boundary. With the current API it must
hand-roll the geometry check before every write:

```rust
// roughly what a real caller writes today
if self.cursor.row >= size.rows {
    return;
}
if self.cursor.col + ch.width > size.cols {
    // pad to the end of the row and move on
    self.push_newline();
    continue;
}
self.push_char(ch);
```

That check duplicates logic the frame already performed internally, and it has
to be kept in step with the frame's own notion of clipping. If the two ever
disagree (for example on a zero-width style continuation), the caller pads or
wraps at the wrong place, with no error to point at it.

## Guide-level explanation

Before:

```rust
if !frame.push_char(ch) {
    // was it the bottom? the right edge? ask the frame's geometry again
}
```

After:

```rust
match frame.push_char(ch) {
    Pushed::Stored => {}
    Pushed::ClippedRow => {
        // the character did not fit this row; finish the layer and advance
        frame.push_newline();
    }
    Pushed::ClippedBottom => {
        // there is no room left in the frame; stop drawing this layer
        break;
    }
}
```

The caller no longer reconstructs a decision the frame already made.

## Reference-level explanation

```rust
/// The result of writing a styled character into a frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pushed {
    /// The character was stored at the cursor and the cursor advanced.
    Stored,
    /// The character did not fit in the remaining columns of the current row.
    /// The cursor is unchanged.
    ClippedRow,
    /// The cursor was already past the last row. The cursor is unchanged.
    ClippedBottom,
}
```

Two shapes are possible and the choice is the substance of this RFC:

1. **Change `push_char`'s return type to `Pushed`.** One method, one meaning.
   Breaks every existing call site that tests the `bool`, but they are all
   testing "did it fit", which maps onto `== Pushed::Stored` exactly.
2. **Keep `push_char -> bool` and add `try_push_char -> Pushed`.** Nothing
   breaks; two methods that differ only in the richness of their answer.

The author's preference is (1): the richer answer is the one the compose-and-
clip use case needs, and a `bool` wrapper that throws the enum back down to
`== Stored` is set dressing. This is recorded as an open question because option
(2) is the realistic fallback if the break is judged too expensive.

Invariants that must hold for either shape: a `Stored` result advances the
cursor by the character's width; `ClippedRow` and `ClippedBottom` leave the
cursor exactly where it was, so a caller may retry after `push_newline`.

## Alternatives

### Keep the `bool` and document the geometry rule more loudly

Rejected as the primary proposal. The rule is already documented; what is
missing is a way to *use* the answer without re-deriving it. Better prose does
not remove the duplicated check.

### Return `Result<(), ClipKind>`

Rejected. Clipping is not an error. A caller that does not care should write
`let _ = frame.push_char(ch);`, and a `Result` invites `?` and `unwrap` on a
path where neither is meaningful.

### Have the frame expose its cursor and let callers compare

Partially already possible, and it is what callers do today. Exposing the
cursor does not remove the duplicate decision; it relocates it.

## Drawbacks

- A public enum is more surface than a `bool`. The enum is small and its three
  variants are exactly the three outcomes of a write, so the surface is honest.
- Option (1) is a breaking change to a method most callers call.

## Open questions

- Option (1) versus option (2) above.
- Should there be a `Pushed::ClippedRow`-equivalent result that also carries
  *how many columns were left*, so a caller can pad in one step instead of
  computing `cols - cursor.col`? Left out of the sketch to keep the enum to
  three honest variants; noted in case the pad case turns out to be common.
