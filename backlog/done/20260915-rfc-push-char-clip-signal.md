# RFC: Give `Frame::push_char` a reason for clipping

- Status: rejected

## Summary

`Frame::push_char` returns `bool`, collapsing two distinct failure causes
("past the right edge of the row" and "no row left") into `false`. This
proposal was to replace the `bool` with a result that names the cause.

Rejected: the information is already available to callers from the public
position API at no real cost, so the breaking change is not worth it. The
catch-up is documentation instead: `push_char`'s rustdoc now spells out that
the two causes are indistinguishable after the fact and shows how to decide
before pushing.

## Motivation

`push_char` clips for two reasons and reports both as `false`. A caller that
wants to treat one cause as normal and the other as an error has to re-derive
the boundary from `next_push_position()` and `size()` on every call, which is
the check the frame already performs internally.

A concrete case: an application that composes a rectangular buffer of cells
and renders it into a `Frame` wants to detect a size mismatch between its
buffer and the frame. Today it cannot ask the frame "did this clip because the
row ended or because the frame ended" and must reconstruct that from the
public position API.

## Guide-level explanation

Before:

```rust
if !frame.push_char(ch) {
    // right edge? no row left? both are `false`.
}
```

After:

```rust
match frame.push_char(ch) {
    Ok(()) => {}
    Err(Clipped::WouldOverflowRow) => { /* wrap the line */ }
    Err(Clipped::RanOutOfRows) => { /* the buffer does not fit */ }
}
```

## Reference-level explanation

```rust
/// Why [`Frame::push_char`] did not store a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clipped {
    /// The character would extend past the right edge of the current row.
    WouldOverflowRow,
    /// There is no row left beneath the current position.
    RanOutOfRows,
}

impl Frame {
    pub fn push_char(&mut self, ch: Char) -> Result<(), Clipped>;
}
```

- The current body checks `self.tail.row < self.size.rows && self.tail.col +
  ch.width <= self.size.cols`. Split it: first test the row, then the column,
  and map the failing test to the matching variant.
- The position still advances by the character's width in both clipped cases,
  exactly as today; only the return type changes.
- `push_newline`, `push_tab`, and the other mutating methods keep their
  signatures. Whether they need the same treatment is left to "Unresolved
  questions".

## Drawbacks

- **Breaking change.** `push_char` is public and returning `Result` forces every
  existing call site to change. tuinix is pre-1.0, so this is acceptable, but
  it is the main cost.
- **Two-armed enum for two cases.** A caller that only cares whether the
  character was stored now writes `.is_ok()` where it wrote a `bool`. This is
  small but it is real friction for the common case.

## Rationale and alternatives

- **Alternative: add `try_push_row(&[Char])`.** A row-atomic API would answer
  the size-mismatch case directly by making the whole row succeed or fail.
  Rejected *as the primary fix* because it does not help callers that push
  character by character, and it is a larger API addition; it can be a
  follow-up (see "Future possibilities").
- **Chosen: keep `bool`, and document the two causes.**
  - The position and size needed to predict a clip are already public
    ([`next_push_position()`], [`size()`], [`Char::width()`]), and the check is
    two expressions, so nothing is actually hidden from a caller today.
  - Predicting *before* pushing is strictly more useful than inspecting the
    return value: a caller that only cares about "the frame ran out of rows"
    writes `if pos.row >= frame.size().rows { ... }` and never looks at the
    result at all, whereas a `Result` return makes every such caller handle a
    value it does not need.
  - `push_char` folds the two causes together, but it is a normal, expected
    outcome for a text-oriented caller (the row simply ended) rather than an
    error, so a `Result` frames the common case as exceptional.
  - Paying a breaking change here would impose a cost on both kinds of caller
    (the ones that want the cause and the ones that are happy predicting it)
    to save the former a two-line check.
- **Alternative: add a predicate such as `can_push_char(ch) -> bool`.** It
  would keep `push_char` unchanged while removing the duplicated check, but it
  only shortens the same two expressions, so it does not earn a new public
  method either.

## Unresolved questions

- None; settled as rejected. Implementation is the documentation catch-up
  described in the Summary.

## Future possibilities

- A row-atomic `try_push_row(&[Char])` (or `push_row`) could still be added if a
  recurring need appears; it would not depend on this proposal. If it ever
  lands and needs to name a cause, the name would be reconsidered then.

[`next_push_position()`]: ../src/frame.rs
[`size()`]: ../src/frame.rs
[`Char::width()`]: ../src/frame.rs
