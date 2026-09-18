# RFC: Provide a "measured" constructor for `Char`

- Status: rejected

## Summary

Add a constructor that takes a `char` and a `Style` and derives the display
width itself — for example `Char::new_measured(ch: char, style: Style)` — so a
caller that cannot depend on a width crate still has a way to ask tuinix to be
as right as tuinix can be about width.

Rejected. tuinix does not measure width, and a constructor that measured some
widths would make that wrongness tuinix's rather than the caller's without
ever making it right. See `## Outcome`.

## Motivation

`Char::new(value: char, width: u8, style: Style)` trusts the caller's `width`.
Its rustdoc says as much: the caller passes the display width, and tuinix does
not compute it.

That is a deliberate choice (tuinix has no Unicode width dependency), but it
leaves the caller with no middle ground. A caller with no width crate has two
options:

1. Pass `1` for everything. This is wrong for East Asian wide characters and
   for combining marks (a zero-width character given width 1 shifts the rest
   of the row), and it is wrong *silently* — the frame stores the cell and the
   terminal wraps or draws a hole.
2. Bring in a width crate itself, pay its dependencies, and thread the width
   through every construction site. Then tuinix's own dependencies still do not
   have what the caller had to add.

A caller that genuinely cannot measure (the realistic case on a constrained
target) ends up doing (1) because it is the only one available, and the
approximation is invisible at the call site: `Char::new(c, 1, style)` reads as
a statement, not as a guess.

## Guide-level explanation

Before, an unmeasurable caller picks a width and hopes:

```rust
// approximately right, and nothing says so at the call site
let ch = Char::new('あ', 1, style);
```

After, the caller states that it cannot measure and lets tuinix decide:

```rust
let ch = Char::new_measured('あ', style);
```

If tuinix itself has no width data, `new_measured` is at least the one place
that documents the approximation it makes, and callers who *can* measure
(`Char::new` with a real width) keep doing exactly what they do today.

## Reference-level explanation

```rust
impl Char {
    /// Build a styled character, choosing its width from the character.
    ///
    /// This is the constructor for callers that do not measure width
    /// themselves. The width is derived from what tuinix knows about `char`;
    /// if tuinix has no width data compiled in, every character is treated as
    /// narrow and wide or combining characters will be misaligned.
    /// [`Char::new`] remains the way to pass a width the caller measured.
    pub fn new_measured(value: char, style: Style) -> Self { /* ... */ }
}
```

The substance of this RFC is *which* widths are compiled in:

- **Zero dependencies, smallest table.** A small inline rule for the common
  ranges (e.g. `0x1100..=0x115F`, `0x2E80..=0xA4CF`, `0xFF00..=0xFF60`) plus
  zero width for combining marks. Covers most real text, stays freestanding,
  and is still an approximation for anything exotic.
- **Optional off-by-default width feature.** `new_measured` uses a real width
  crate when a `width` feature is on and falls back to the inline rule
  otherwise. Real accuracy for callers who want it, no dependency for callers
  who do not.

The author leans toward the first: the inline rule is what a caller writes
anyway when it cannot measure, and putting it in tuinix means it is written
once and documented as an approximation instead of re-derived by each caller.
The second is recorded as an alternative because the accuracy ceiling of the
first is real.

## Alternatives

### Do nothing; document that the caller owns width

Nothing changes, and callers keep passing `1` with no signal at the call site
that it is an approximation. This turned out to be the right answer; the
Motivation above is the description of why it looks like a problem, and the
Outcome explains why it is not one that tuinix should solve by measuring.

### Require the caller to measure, never provide a fallback

Rejected as too strict. It forces a width crate onto every caller, including
ones that would rather be slightly wrong than add a dependency, which is the
choice tuinix's own dependency policy already made.

### Take `unicode-width` unconditionally

This is the only option that would make `new_measured` actually correct, and it
is the reason the proposal fails: tuinix has no width dependency, and the whole
point of `Char::new` trusting a declared width is that the decision is not
tuinix's to make. Recorded here in case that stance ever changes.

## Drawbacks

- A second `Char` constructor is one more thing to choose between.
- Any inline width table is wrong for *some* text, and shipping one makes that
  wrongness tuinix's rather than the caller's. This is the main cost, and it
  is why the zero-width case deserves care: a wrong wide width draws too many
  columns, a wrong zero width draws none, and only one of those is purely
  cosmetic.

## Outcome

Rejected. Both shapes the RFC considered amount to tuinix shipping a width
table, and a table that is only approximately right relocates the error from
the caller to tuinix without removing it. The `Char` documentation already
settles the responsibility question in one direction — the declared width is
the only source of that information, and deciding it is the caller's job — and
adding a constructor that measures would contradict that while buying no
accuracy that a caller could not get from a width crate itself.

The Motivation is not wrong about the problem: a caller that cannot measure
ends up passing `1`, and nothing at the call site says so. But the fix for that
is the existing wording on `Char` and `Char::new()`, which states outright that
tuinix does not measure and cannot detect a mismatched width. Repeating it in a
fourth place would add another wording to keep in step without giving a caller
new information, so no documentation change was made either.

The optional off-by-default `width` feature was rejected with the rest: a
feature flag changes which crate measures, not whether tuinix should be the one
doing it.

No change landed. The scope is unchanged from what is described above.

## Unresolved questions

- None; settled as rejected when the item was decided.
