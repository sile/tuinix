# RFC: Add function keys to `KeyCode`

- Status: accepted

## Summary

`KeyCode` has no way to represent the function keys (F1–F12), so the sequences
that terminals send for them (`ESC [ 11 ~` through `ESC [ 24 ~`, with holes) are
not mapped to any key. Add a single `KeyCode::F(u8)` variant and the standard
sequences that produce it, so an application can bind F1–F12 like any other key.

`ESC [ 13 ~` (F3 on some terminals, an extra Enter on others) is deliberately
left unmapped, and SS3-style F-keys are out of scope; both are discussed below.
Shift/Ctrl/Alt-modified forms (`ESC [ 15 ; 2 ~` = Shift+F5, and friends) are
included: they share the same table and the same decoding as the bare forms.

## Motivation

A terminal sends function keys as `~`-terminated CSI sequences. tuinix already
maps the low numbers (`ESC [ 1 ~` Home … `ESC [ 8 ~` End) in
`parse_special_key_simple`, but F1–F12 live in the same numbering space and are
unmapped, so they arrive as nothing at all and are silently discarded by the
`~`-sequence consumer.

The gap is visible to any application that offers F-key bindings. tuinix itself
is the reproduction: feed `ESC [ 15 ~` (F5) to [`InputDecoder`] and no [`Input`]
is produced — the bytes are consumed and dropped. There is no way for a caller
to recover it, because the information is not in `Input` either. This is not a
parser defect (the sequence is correctly recognized and consumed); it is a
missing variant plus a missing mapping.

[`InputDecoder`]: ../src/input.rs
[`Input`]: ../src/input.rs

## Guide-level explanation

Before, pressing F5 does nothing that the application can observe:

```rust
match input {
    Input::Key(KeyInput { code: KeyCode::Char('q'), .. }) => quit(),
    // F1–F12 cannot be written here: there is no KeyCode for them.
    _ => {}
}
```

After, function keys are ordinary key codes:

```rust
match input {
    Input::Key(KeyInput { code: KeyCode::F(1), .. }) => help(),
    Input::Key(KeyInput { code: KeyCode::F(5), .. }) => reload(),
    _ => {}
}
```

Ctrl and Alt modifiers are carried by `KeyInput` exactly as they already are for
arrows, so `Ctrl+F1` is `KeyCode::F(1)` with `ctrl == true`. The modified forms
use the same `ESC [ <num> ; <mod> ~` shape and the same modifier bits
(Alt = `&0x2`, Ctrl = `&0x4`) as the modified arrows and `~` keys that already
exist, so `ESC [ 15 ; 2 ~` is `F(5)` with `alt == true`.

## Reference-level explanation

```rust
pub enum KeyCode {
    // ... existing variants ...

    /// A function key, `F(1)` through `F(12)`.
    ///
    /// Values outside `1..=12` have no producer in the parser; they exist only
    /// because the range is not enforced by the type.
    F(u8),
}
```

The mapping added in `parse_complex_csi_key` (the `ESC [ <num> ~` family):

| Sequence        | Key          |
| --------------- | ------------ |
| `ESC [ 11 ~`    | `F(1)`       |
| `ESC [ 12 ~`    | `F(2)`       |
| `ESC [ 13 ~`    | *(not mapped — see below)* |
| `ESC [ 14 ~`    | `F(4)`       |
| `ESC [ 15 ~`    | `F(5)`       |
| `ESC [ 17 ~`    | `F(6)`       |
| `ESC [ 18 ~`    | `F(7)`       |
| `ESC [ 19 ~`    | `F(8)`       |
| `ESC [ 20 ~`    | `F(9)`       |
| `ESC [ 21 ~`    | `F(10)`      |
| `ESC [ 23 ~`    | `F(11)`      |
| `ESC [ 24 ~`    | `F(12)`      |

Note that the xterm numbering is **not** the arithmetic sequence one would
expect: apart from `13` (skipped, see below), `16` and `22` do not correspond to
any key in this set. So `F(n) = n - 10` holds for `11 ~ = F(1)` through
`15 ~ = F(5)`, but `17 ~` is F6 (not F7) and `24 ~` is F12 (not F14). The table
above is the source of truth: it is not generated from a formula.

The same mapping applies to the modified form `ESC [ <num> ; <mod> ~`, which
carries the modifier in `KeyInput`:

| Sequence            | Key                 |
| ------------------- | ------------------- |
| `ESC [ 11 ; 2 ~`    | `F(1)` + Alt        |
| `ESC [ 15 ; 5 ~`    | `F(5)` + Ctrl+Alt   |
| `ESC [ 24 ; 2 ~`    | `F(12)` + Alt       |

The unmapped numbers (`13`, `16`, `22`, and anything else) are discarded in
both forms, exactly as the bare form is.

Structural details that the implementation must respect:

- The handling belongs in the `~`-consuming path in `parse_complex_csi_key`,
  which already finds the terminator with `find_tilde_terminator` and returns the
  consumed length. The new decoding replaces the discard for the listed numbers
  and leaves the discard in place for every other number.
- The parameter is multi-digit and may be followed by `;` and a modifier, so the
  current dispatch on `bytes[2]` cannot be used directly. The parser must split
  the parameter text between `bytes[3]` and the terminator on `;` and parse the
  leading number. This is the same hole that was fixed for arrows and `~`
  sequences; the new code must not reintroduce a fixed-length assumption.
- Sequence length varies (`11~` is 5 bytes, `15~` is 6, `15;2~` is 8), so the
  returned consumed length must come from the discovered terminator, not a
  constant.

## Drawbacks

- **`F(0)` and `F(200)` are representable.** `Char(char)` has the same
  property, and matching `KeyCode::F(n)` in an application is easier than 12
  variants, so the loss is accepted. The rustdoc states the real range.
- **Another public variant** means another arm in exhaustive `match`es in
  downstream crates. The alternative (12 variants) is worse, not better: it is
  the same breakage with more names to write.
- **`ESC [ 13 ~` stays unmapped.** It is F3 on some terminals and an extra
  Enter on others (notably older xterm configurations), so mapping it to
  `F(3)` would be wrong on those terminals. Leaving it in the discard path is
  the safe default, and is treated as settled here rather than as an open
  question.
- **Terminal coverage is not universal.** Some terminals send F1–F12 as SS3
  (`ESC O P` …) instead of CSI `~`. Adding only the CSI form covers the common
  xterm-style case; the SS3 form is out of scope here (see "Future
  possibilities").

## Rationale and alternatives

- **Alternative: 12 named variants (`F1` … `F12`).** Self-documenting, but
  three times the enum surface for no expressive gain, and it would make the
  parser's table twelve lines of nearly identical code. `F(u8)` mirrors
  `Char(char)`, which is the existing precedent for "a family of keys with a
  payload".
- **Alternative: `KeyCode::Function(u8)`.** Equivalent; `F` is shorter and
  unambiguous next to the other variants.
- **Alternative: do nothing.** F-keys remain unobservable. Anyone who needs
  them has to re-implement CSI parsing outside tuinix, which is exactly the
  layering the library exists to prevent.
- **Alternative: map `ESC [ 13 ~` to `F(3)`.** Rejected: the same sequence
  means "extra Enter" on some terminals, so one of the two interpretations would
  be wrong no matter which is chosen. Not mapping it keeps both terminals
  correct — the sequence is consumed and discarded, as it is today.
- **Interaction with "unknown keys".** tuinix currently discards unrecognized
  sequences rather than surfacing them. This RFC does not change that; it moves
  a known set of sequences from "discarded" to "mapped". A general
  `KeyCode::Unrecognized`-style escape hatch is a separate question and is not
  required to make F-keys work.

## Decision

The three questions left open in the draft were settled before implementation:

- **Shape:** `KeyCode::F(u8)` (not 12 named variants).
- **Modifiers:** included in this change, using the existing `ESC [ <num> ;
  <mod> ~` decoding. The bare and modified forms share one table, so splitting
  them would mean touching the same parser branch twice.
- **SS3 function keys** (`ESC O P` …): out of scope. If a real terminal turns
  out to need them, `F(u8)` gives them a place to land without another enum
  change; revisit then.

## Unresolved questions

None. The remaining choices (SS3, unknown-key surfacing, wider `~`-family
coverage) are recorded under "Future possibilities" as follow-ups rather than
left open here.

## Future possibilities

- SS3 F-keys (`ESC O P`, `ESC O Q`, …) mapping to the same `F(u8)` variants.
- A general answer for "keys tuinix does not know", so an application can see
  that something arrived instead of guessing from its absence.
- Extending `~`-family coverage beyond F-keys (e.g. `ESC [ 25 ~`/`26~`,
  `ESC [ 28 ~`/`29~` on some terminals) under the same numbering table.

## Outcome

Implemented: `KeyCode::F(u8)` was added and the `~`-consuming path in
`parse_complex_csi_key` now decodes `ESC [ <num> ~` and `ESC [ <num> ; <mod> ~`
for the numbers in the table above, in both bare and modified forms.
