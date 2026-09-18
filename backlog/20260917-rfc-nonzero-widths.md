# RFC: Make the tab width a non-zero type

- Status: accepted

## Summary

Change `Position::next_tab_stop()` to take a `NonZeroUsize` tab width, so the
"greater than `0`" rule moves from an `assert!` plus a "Panics if `0`" doc
clause into the signature. Leave `Char::width()` as `usize`.

## Motivation

A tab width of `0` is meaningless, and `next_tab_stop()` currently enforces that
with an `assert!` and documents it as a panic. That is a rule the caller has to
remember from prose. A non-zero type makes the invalid value impossible to
construct, removes the panic, and lets the rustdoc drop its "Panics if" clause —
the promise moves from documentation into the signature, at no cost to the call
site, since a tab width is normally a `const`.

`Char::width()` makes a superficially similar "`1` or more" promise, and an
earlier draft of this proposal changed both. That framing is dropped; see
"Why `Char::width()` stays `usize`" below.

## Guide-level explanation

Today, a zero tab width is rejected at the point of use:

```rust
// panics: tab width must be greater than 0
at = at.next_tab_stop(0);
```

With a non-zero type, a zero tab width cannot be written down:

```rust
use std::num::NonZeroUsize;

// the application owns the tab width, and writes it down once
const TAB_WIDTH: NonZeroUsize = NonZeroUsize::new(4).expect("4 is not 0");

at = at.next_tab_stop(TAB_WIDTH);
```

The literal is built once, at the application's own constant, and every call
site passes that constant. tuinix does not supply a tab width of its own: it has
no way to know what an application considers a tab stop, the same way it does
not read the terminal's palette.

## Reference-level explanation

```rust
impl Position {
    /// The next tab stop at or after this position, stepping by `tab_width` columns.
    pub const fn next_tab_stop(self, tab_width: NonZeroUsize) -> Self;
}
```

`Char::new`, `Char::width()`, and `Position::advance()` keep their `usize`
widths and are not touched.

## Drawbacks

- The tab width can no longer be passed as a bare literal. That cost lands once,
  on an application's own `const`, because a tab width is a value an application
  states rather than derives.
- A `NonZeroUsize` is less specific a name than the parameter it replaces. The
  rustdoc for `next_tab_stop()` has to say "tab width", since the type does not.

## Rationale and alternatives

### Why not leave the tab width as `usize`?

The status quo has no migration cost and the `assert!` already works. It is
rejected only if the prose-plus-panic style is judged worse than building the
constant, which is a value judgment rather than a correctness one. If nothing
here motivates the change, rejecting this RFC is a reasonable outcome.

### Why `Char::width()` stays `usize`

An earlier draft treated `Char::width()` and the tab width as "the same
promise" and changed both. On inspection they are different kinds of rule:

- `Char::width()` is an invariant fixed when the character is built. It is the
  width the caller declared, and `Char::new()` already rejects `0`.
- The tab width is a precondition on an argument, passed on every call.

Only the second is a rule that an `assert!` is holding up. `Char::width()` has
no runtime check to remove at its point of use, and `Char::new()` returns `None`
for control characters anyway, so making its width non-zero would remove one of
two reasons for that `None` while leaving the caller still handling `None` — a
partial gain at the price of `NonZeroUsize::new(1).expect(...)` on the most
frequently written constructor in tuinix. The two rules are not one
representation split in two, so there is no inconsistency in changing only the
tab width.

### Why not supply a default or a few common widths?

A tab width is in practice `2`, `4`, or `8`, which tempts a library constant.
It is declined because tuinix cannot know what an application considers a tab
stop, in the same way it does not read the terminal's palette. A single
`TAB_WIDTH` would only serve the applications that happen to share that value,
while suggesting tuinix has an opinion it does not have; a set of them would be
worse. The application states its own constant once; that is the whole cost.

### Rejected alternative: `NonZeroU8` for the tab width

A tab width is a small number in practice, so `u8` would fit. It is rejected
because no other width in tuinix is `u8`, and the memory saved does not apply:
the tab width is passed as an argument rather than stored in a frame.

### Rejected alternative: a `TabWidth` newtype

A newtype (`TabWidth(NonZeroUsize)`) was considered, so that "tab width" would
be visible in the type rather than only in the parameter name. It is declined
because the name is the only thing it adds. `next_tab_stop()` computes with its
argument (`(tab_width - self.col % tab_width) % tab_width` and
`advance(tab_width)`), so a newtype would have to hand its inner value back out
at every step, and tuinix has no other newtype that pays that cost for a name.
`NonZeroUsize` reuses a familiar type, and `next_tab_stop()`'s own doc supplies
the name.

## Unresolved questions

None. The choice was between `NonZeroUsize` and a newtype, and the newtype was
rejected above.

## Future possibilities

- If a `Char` width newtype is introduced later — for instance to carry a
  `saturating_add` when stacking widths — the tab width could move to it as
  well, so that widths have one name. Any such newtype would subsume the
  reasoning above, since the argument would no longer be a bare non-zero number.
- Non-zero types for any other "must be positive" value that appears later
  (for example a region extent) would follow the same pattern.
