# RFC: Make character and tab widths non-zero types

- Status: draft

## Summary

Represent the "`1` or more" rule for character widths and tab widths in the
type, using a non-zero integer instead of a plain `usize` plus a runtime check.

## Motivation

Two values in tuinix are widths that cannot be zero, and both currently say so
in prose and enforce it at runtime:

- [`Char::width()`](crate::Char::width) documents "always `1` or more", and
  [`Char::new()`](crate::Char::new) returns `None` when the width is `0`.
- `Frame::push_tab()` asserts that the tab width is greater than `0`, and its
  rustdoc carries a "Panics if `0`" clause. (Under
  `20260917-rfc-frame-write-model.md`, this becomes
  `Position::next_tab_stop()`.)

A rule that is stated in a comment and checked with `assert!` or a `None` return
is a rule the caller has to remember. A non-zero type makes the invalid value
impossible to construct, removes the panic, and lets the rustdoc drop its
"Panics if" clause — the promise moves from documentation into the signature.

## Guide-level explanation

Today, a zero width is rejected at the point of use:

```rust
// None: width must be 1 or more
let ch = Char::new('あ', 0, style);

// panics: tab width must be greater than 0
frame.push_tab(0);
```

With non-zero types, a zero width cannot be written down:

```rust
// compile error: 0 is not a non-zero value
let ch = Char::new('あ', NonZeroUsize::new(0), style);

// the tab width is a value that is already known to be non-zero
let tab = NonZeroUsize::new(4).expect("4 is not 0");
at = at.next_tab_stop(tab);
```

The cost is visible in that second example: every call site that passes a
literal has to build the type first. That cost is the reason this proposal is
tracked on its own rather than folded into the write-model change.

## Reference-level explanation

```rust
impl Char {
    /// Makes a new styled character with the given width.
    ///
    /// Returns `None` when `value` is a control character. A width of `0` is
    /// not representable, so it needs no arm here.
    pub const fn new(value: char, width: NonZeroUsize, style: Style) -> Option<Self>;

    /// The number of terminal columns this character occupies.
    pub const fn width(self) -> NonZeroUsize;
}

impl Position {
    /// The next tab stop at or after this position, stepping by `tab_width` columns.
    pub const fn next_tab_stop(self, tab_width: NonZeroUsize) -> Self;

    /// The position this far to the right.
    pub const fn advance(self, width: NonZeroUsize) -> Self;
}
```

`Char::BLANK` keeps width `1`. Its `width` field becomes the non-zero type, but
the constant value does not change.

### Where `usize` still appears

`Position`'s coordinates and `Size`'s extents stay `usize`; a `row` or `col` of
`0` is meaningful and must stay representable. Only widths become non-zero. That
leaves arithmetic between a width and a coordinate as `width.get() + col` or an
`advance(width)` call that does the conversion once, rather than every caller
doing it.

### Rejected alternative: `NonZeroU8` for the tab width

A tab width is a small number in practice, so `u8` would fit. Pushing it to
`NonZeroU8` is rejected for two reasons: `advance()` and `Char::width()` deal in
`usize`, so every use would need a cast back; and the memory saved by a smaller
type does not apply, since the tab width is passed as an argument rather than
stored in a frame.

## Drawbacks

- Every call site that passes a literal width becomes
  `NonZeroUsize::new(n).expect(...)` or carries a `const`. `Char::new` in
  particular is a widely used constructor, and this makes it heavier to call.
- `Char::new` returns `Option` for the control-character case. If a non-zero
  width still leaves a `None` arm, the type change removes only one of the two
  reasons and the caller still handles `None`.

## Rationale and alternatives

### Why not leave both as `usize`?

The status quo has no migration cost and the runtime checks already work. It is
rejected only if the prose-plus-check style is judged worse than the call-site
cost, which is a value judgment rather than a correctness one. If nothing here
motivates the change, rejecting this RFC is a reasonable outcome.

### Why not change only the tab width?

Because the two widths make the same promise. Making the tab width non-zero
while `Char::width()` stays `usize` would leave "a width is at least 1" split
across two representations — enforced by the type in one place and by
convention in the other — which is the inconsistency this proposal exists to
avoid. Either both change or neither does.

### Why not change only `Char::width()`?

Symmetric to the above, and worse in one respect: `push_tab`'s `assert!` would
remain the only place a zero width is caught at runtime, while the type system
handled the other.

## Unresolved questions

- Should this be adopted at all, given the call-site cost on `Char::new()`? The
  change is only worth making if the type-level guarantee is judged more
  valuable than the convenience of `Char::new('x', 1, style)`.
- `NonZeroUsize` versus a newtype (`CharWidth(NonZeroUsize)`) that could carry
  the doc and any methods. A newtype adds a type to learn; `NonZeroUsize` reuses
  a familiar one at the price of a less specific name.

## Future possibilities

- If a width newtype is introduced, it is the natural home for a
  `saturating_add` when stacking widths, which currently has to be written at
  each call site.
- Non-zero types for any other "must be positive" value that appears later
  (for example a region extent) would follow the same pattern.
