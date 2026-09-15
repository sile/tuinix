# RFC: Rename `Frame::next_push_position`

- Status: draft

## Summary

Rename `Frame::next_push_position()` to `Frame::push_position()`, so that the
getter for the write position reads like the other getters on the type
(`Frame::size()`) instead of naming an operation it does not perform.

## Motivation

`Frame` exposes two getters that answer questions about its own state:

```rust
pub fn size(&self) -> Size;
pub fn next_push_position(&self) -> Position;
```

`size()` is a plain noun. `next_push_position()` embeds the name of a different
method (`push_char()`) inside a getter, which reads as though calling it were
part of a push, or as though it advanced the position. Neither is true: it is a
`&self` accessor that returns a value and changes nothing.

The `next_` prefix is also the only thing distinguishing the name from the
field it exposes. Internally the value is `Frame::tail`, the position after the
last pushed character, and `next_push_position()` is the name given to the
*upcoming* write position rather than the previous one. A reader has to hold
both ideas at once to know that the value returned is where the next character
goes, and the name does not say which of the two it is without reading the doc.

The tension is with the writers. `push_char()`, `push_newline()`, and
`push_tab()` are verbs, and one reasonable reading of `next_push_position()` is
that it is deliberately phrased as a verb group to match them. But it is a
getter, and the crate has already settled that its getters are nouns
(`size()`, `next_push_position()` is the exception, not the rule).

## Guide-level explanation

The write position is read like any other frame property:

```rust
let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
frame.push_char(tuinix::Char::new('a', 1, tuinix::Style::new()).expect("valid char"));

assert_eq!(frame.push_position().col, 1);
frame.push_char(tuinix::Char::new('b', 1, tuinix::Style::new()).expect("valid char"));
assert_eq!(frame.push_position().col, 2);
```

The name pairs with the writers by sharing the `push` token, without claiming
that the getter pushes anything.

## Reference-level explanation

`Frame::next_push_position()` is renamed to `Frame::push_position()`. The
signature and behavior are unchanged:

```rust
pub fn push_position(&self) -> Position;
```

The name still has to be distinguishable from the display cursor, which is the
`cursor` argument of `Frame::render()` and is unrelated (the doc for
`next_push_position()` says so today and the sentence carries over unchanged).
`push_position()` is chosen over `position()` for exactly that reason: the bare
name would read as "the frame's position" and invite the cursor confusion.

## Drawbacks

This is a breaking rename with no functional benefit. Every caller must be
updated, and the old name is arguably more explicit about being the position
for the *next* character than the new one is.

The new name is also shorter, so a reader who does not know the `push_char()`
family loses a hint. That hint is recovered from the doc, which has to be read
in either case to know that the value is the upcoming position rather than the
last written one.

## Rationale and alternatives

**Do nothing.** The name is defensible: it is the only getter that names the
operation the value feeds, and renaming it is pure churn. The argument for
renaming is consistency, and consistency alone is a real but weak reason for a
breaking change — this RFC would be reasonable to postpone until another change
to the `Frame` API makes it cheap to include.

**`push_cursor()`.** Avoids the `position`/`cursor` overlap concern by lining up
with the terminal cursor, but the frame itself has no cursor; only `render()`
has one. The name would suggest a state the frame does not hold.

**Keep `next_push_position()` on the getter and add `push_position()` as an
alias.** Two names for one getter is worse than either name alone, and an alias
has to be deprecated eventually, which is the same breaking change with extra
steps.

**Rename the writers instead** (for example `push_char()` to `write_char()`) so
that the getter no longer echoes an operation. This does not help: `push_char`
is the name the rest of the API and the doc use, and the getter would still
contain the `next_` prefix that is the actual problem.

## Unresolved questions

- If `Frame::next_push_position()` is renamed, should the internal field `tail`
  be renamed to match? It is not public, so this is a readability question for
  the implementation rather than part of the proposal.
