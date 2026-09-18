---
Created: 2026-09-17
Status: rejected
---

# RFC: Tell the caller *why* `push_char` clipped

## Summary

Give `Frame::push_char` a return value that distinguishes "the character did
not fit because it would wrap off the bottom" from "the character did not fit
because it would overflow the right edge", or add a `try_push_char` that
returns that distinction while `push_char` keeps its current `bool`.

Rejected, for the same reasons as the earlier
`done/20260915-rfc-push-char-clip-signal.md`: the position and size needed to
predict a clip are already public, predicting before the write is more useful
than inspecting a result after it, and naming a cause would not save the
caller the geometry check anyway. That earlier item settled this by improving
`push_char`'s rustdoc; this one proposed the same change again without a new
argument, so it is closed without reopening the decision. Nothing about the
layer-composition case in Motivation is new either — see
`20260917-rfc-frame-random-access-write.md`, which is the standing answer to
it and stays open.

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

- None; settled as rejected when the item was closed as a duplicate of the
  earlier item.

## Outcome

Closed as a duplicate of `done/20260915-rfc-push-char-clip-signal.md`, which
reached the same conclusion with the same reasoning. No change landed: the
return value of `push_char` is still `bool`, and the documentation catch-up that
the earlier item describes is already in place.

The scope is unchanged from what is described above.

One thing this item did surface is real and is not settled here: the
cursor-only write API is a poor fit for composing a screen in layers, which is
the problem `20260917-rfc-frame-random-access-write.md` proposes to solve. That
item is the one to follow for the layer-composition case.
