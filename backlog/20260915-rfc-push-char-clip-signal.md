# RFC: Give `Frame::push_char` a reason for clipping

- Status: draft

## Summary

`Frame::push_char` returns `bool`, collapsing two distinct failure causes
("past the right edge of the row" and "no row left") into `false`. Replace the
`bool` with a result that names the cause, so a caller can distinguish an
expected wrap from a genuine size mismatch.

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
- **Alternative: keep `bool`, document the two causes.** The rustdoc already
  states both causes, so this is the do-nothing option. It leaves the boundary
  check duplicated in every caller that needs to tell them apart, which is the
  situation this proposal exists to remove.

## Unresolved questions

- Should `push_newline` / `push_tab` also report *why* they moved or clipped,
  or is `push_char` the only method where the cause matters to a caller?
- Is `Clipped` the right name, or should it be a closed enum on the error path
  named for the frame (`ClipCause`)?

## Future possibilities

- A row-atomic `try_push_row(&[Char])` (or `push_row`) could be added on top,
  using the same `Clipped` classification.
