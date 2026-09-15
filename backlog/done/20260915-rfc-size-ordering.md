# RFC: Stop deriving `Ord` for `Size`

- Status: accepted

## Summary

Stop deriving `PartialOrd` and `Ord` for `Size` and keep only `PartialEq` and
`Eq`. The derived order compares `rows` first and `cols` second, which is an
arbitrary tie-break rather than a usable order, and exposing it invites a
caller to write a comparison that looks like an area comparison but is not one.

## Motivation

`Size` derives `PartialOrd, Ord` alongside `PartialEq, Eq, Hash`:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Size {
    pub rows: usize,
    pub cols: usize,
}
```

The derived order is lexicographic on the field order, so
`Size { rows: 2, cols: 100 } < Size { rows: 3, cols: 1 }`. That statement has no
meaning a reader can act on: a display area is not naturally ordered, and the
comparison only reflects the order the fields happen to be declared in. The
same applies to a `BTreeMap<Size, _>` or a `sort()` of sizes, which would
produce an ordering with no interpretation.

The concrete failure mode is the area comparison. "Larger terminal" is a
plausible thing for an application to ask, and `a < b` compiles and always
answers, but it answers the wrong question whenever the two sizes differ in
both dimensions in opposite directions — the exact case that matters. A
`PartialOrd` that cannot be used to compare sizes is worse than no
`PartialOrd`, because it produces a plausible-looking answer instead of a
compile error.

`Size` is also the odd one out in this respect. The crate derives no `Ord` for
`Region`, which has the same shape (a position plus a size) and would be just as
meaningless to order, so the current set of derives is not a deliberate policy
applied across the geometry types.

## Guide-level explanation

`Size` supports equality but not ordering:

```rust
let a = tuinix::Size { rows: 24, cols: 80 };
let b = tuinix::Size { rows: 24, cols: 80 };
assert_eq!(a, b);

// An application that needs to know whether the display grew compares the
// fields itself, so the code says which comparison it means:
let grew = b.rows > a.rows || b.cols > a.cols;
```

Storing sizes in a `BTreeMap` or sorting them is no longer possible. A caller
that was using `Size` as an ordered key sorts by whatever it actually cares
about (`rows`, `cols`, or `rows * cols`) at the call site, where the intent is
visible.

## Reference-level explanation

`Size` keeps `Debug, Default, Clone, Copy, PartialEq, Eq, Hash` and loses
`PartialOrd, Ord`. `Position` keeps its derives unchanged, because its order is
row-major and therefore matches reading order; the point of this proposal is
not to remove ordering everywhere, only where the derived order has no meaning.

## Drawbacks

This is a breaking change for any caller that relies on the derived order, for
example by using `Size` as a key in an ordered collection or by sorting a slice
of sizes. The comparison is also occasionally *usable* even though it is not
meaningful: a caller that only ever compares sizes along one dimension gets the
right answer today, and will have to spell the comparison out after this change.

Both costs are paid once, by adding the comparison the caller actually means
and getting a better answer for it.

## Rationale and alternatives

**Keep the derives.** The argument for keeping them is that a total order is a
harmless extra capability: nobody is forced to use it. The counter-argument is
that the capability is a trap rather than a tool — the compiler accepts
`area_a < area_b` and the result is silently wrong for the cases that matter —
and that an unused capability still has to be maintained and documented.

**Keep `PartialOrd` and drop `Ord`.** This removes the `BTreeMap` and `sort()`
uses while leaving the `<` operator that is the actual hazard, so it is worse
than either extreme: it still permits the wrong comparison and adds an
asymmetry between the two traits that a reader has to reason about.

**Implement `Ord` meaningfully (by area, or by `rows` then `cols` with a
documented tie-break).** A meaningful order would have to be a documented
decision about what "larger" means for a display area, and even then it would
be a guess about the caller's intent. A caller that wants an area comparison
writes `a.rows * a.cols < b.rows * b.cols`; a caller comparing one dimension
writes that comparison. Neither needs an `Ord` on `Size`.

**Keep `Ord` and document it as lexicographic.** Documenting the order does not
make it useful, and it still leaves the area-comparison mistake available.

**Give `Size` an `area()` helper.** An `area()` method would give the area
comparison a home, but it would also turn "the area of a display region" into a
property of `Size`, contradicting the argument for removing `Ord` in the first
place: the caller is the one that knows which comparison it means, and a caller
that wants an area writes `a.rows * a.cols < b.rows * b.cols`. The helper is
also not something most applications need — a TUI more often asks whether a
region fits inside another than which of two regions is larger. A helper added
now cannot be taken away later, so it is better to leave it out and add it in a
follow-up if a real need appears.

## Outcome

Implemented in [#31](https://github.com/sile/tuinix/pull/31) (merged as
`b216395`). `Size` now derives `Debug, Default, Clone, Copy, PartialEq, Eq,
Hash`; `PartialOrd` and `Ord` are gone, and `Position` is unchanged. Scope
unchanged from the Decision section: no `area()` helper is added.

## Unresolved questions

None. The question of an area helper was settled as part of accepting this
proposal: no helper is added.

## Future possibilities

- An `area()` method on `Size`, if applications turn out to need area
  comparisons often enough that writing the multiplication at each call site is
  a burden.
