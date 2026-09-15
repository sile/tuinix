# RFC: Rename `Frame::next_push_position` to `Frame::next_position`

- Status: accepted

## Summary

Rename `Frame::next_push_position()` to `Frame::next_position()`, so that the
getter for the write position reads like the other getters on the type
(`Frame::size()`) instead of embedding the name of an operation it does not
perform.

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

The fix keeps the `next_` that carries the meaning (the position has not been
written to yet) and drops the `push_` that names an operation the getter does
not perform. The result is a noun phrase that says what the value is, not what
it is for.

## Guide-level explanation

The write position is read like any other frame property:

```rust
let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
frame.push_char(tuinix::Char::new('a', 1, tuinix::Style::new()).expect("valid char"));

assert_eq!(frame.next_position().col, 1);
frame.push_char(tuinix::Char::new('b', 1, tuinix::Style::new()).expect("valid char"));
assert_eq!(frame.next_position().col, 2);
```

The `next_` prefix repeats what the doc says in words: the value is the
position the following write will use. The name shares no vocabulary with the
writers, so it cannot be mistaken for one of them.

## Reference-level explanation

`Frame::next_push_position()` is renamed to `Frame::next_position()`. The
signature and behavior are unchanged:

```rust
pub fn next_position(&self) -> Position;
```

The name still has to be distinguishable from the display cursor, which is the
`cursor` argument of `Frame::render()` and is unrelated (the doc for
`next_push_position()` says so today and the sentence carries over unchanged).
`next_position()` is chosen over `position()` for exactly that reason: the bare
name would read as "the frame's position" and invite the cursor confusion, and
the `next_` prefix makes clear that it is the upcoming position rather than the
current one.

The internal field `Frame::tail` is left as it is. It is private, and the name
is accurate for what the implementation stores: the position after the last
written character, not the position the next write will use. The two readings
coincide for every state a frame can be in, but keeping the field name distinct
from the getter keeps the two concepts from looking interchangeable.

## Drawbacks

This is a breaking rename with no functional benefit. Every caller must be
updated, and the old name is arguably more explicit about being the position
for the *next* character than the new one is.

The new name is also shorter, so a reader who does not know the `push_char()`
family loses a hint. That hint is recovered from the doc, which has to be read
in either case to know that the value is the upcoming position rather than the
last written one. `next_position()` also names a feature of the frame rather
than an interaction with it, so the type's listing shows two state getters and
three writers instead of four writers of uncertain meaning; that is the
intended effect, but it is a change in how the type reads, not a pure
clarification.

## Rationale and alternatives

**Do nothing.** The name is defensible: it is the only getter that names the
operation the value feeds, and renaming it is pure churn. The argument for
renaming is consistency, and consistency alone is a real but weak reason for a
breaking change. It is the runner-up, and would win if the crate still promised
the old name to callers; with the rename treated as free, the naming problem
below tips the balance.

**`push_position()`.** Keeps the `push` token the writers use, which reads as
intentional alignment at first. It is the opposite: a getter and a group of
verbs sharing a prefix makes the same word mean two things. `push_position()`
is one letter away from `push_char()` and is shaped like an imperative
(`push_position(x)` would be plausible if it took an argument), so a reader
skimming the type sees a fourth writer rather than a getter. Sharing vocabulary
with the writers is the problem being fixed, not a feature to preserve.

**`pending_position()` / `insertion_position()`.** Both are accurate noun
phrases, but `pending` has no other use in the crate and `insertion` collides
with `KeyCode::Insert`. `next_` already carries the "not written yet" meaning
with vocabulary the type uses elsewhere.

**`push_cursor()`.** Avoids the `position`/`cursor` overlap concern by lining up
with the terminal cursor, but the frame itself has no cursor; only `render()`
has one. The name would suggest a state the frame does not hold.

**Keep `next_push_position()` on the getter and add `next_position()` as an
alias.** Two names for one getter is worse than either name alone, and an alias
has to be deprecated eventually, which is the same breaking change with extra
steps.

**Rename the writers instead** (for example `push_char()` to `write_char()`) so
that the getter no longer echoes an operation. This does not help: `push_char`
is the name the rest of the API and the doc use, and the getter would still
contain the `next_` prefix that is the actual problem.

## Unresolved questions

None. The internal field `tail` keeps its name; see the reference-level
explanation.
