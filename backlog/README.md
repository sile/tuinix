# backlog

This directory holds design proposals (RFCs) and, in the future, other
unprocessed work items such as TODOs and bug reports for tuinix.

## Purpose

An RFC here records *how a decision was reached*, not *what the current
specification is*.

The authoritative specification is:

- the rustdoc comments in `src/` for the public API, and
- the documents under `docs/` for concepts and recipes.

RFCs exist so that future maintainers can reconstruct the reasoning behind a
change. They are not a spec and must not be treated as one. Code and `docs/`
must stand on their own and must **not** reference a file in `backlog/`; if a
decision matters to a reader, write it where the reader is looking instead.

The rule is one-way: `src/` and `docs/` must never point at a file under
`backlog/`, because those are the artifacts users read and they have to make
sense without it. The reverse is fine and expected — a proposal may cite
`src/` and `docs/` freely to explain the code and docs it wants to change.

## Layout

```
backlog/
  README.md                       # this file: process, naming, states
  00000000-rfc-template.md        # template for a new RFC
  00000000-bug-template.md        # template for a new bug report
  YYYYMMDD-rfc-slug.md            # an open RFC
  YYYYMMDD-bug-slug.md            # an open bug report
  done/
    YYYYMMDD-rfc-slug.md          # a settled RFC
    YYYYMMDD-bug-slug.md          # a settled bug report
```

Only two directories exist: `backlog/` for open items and `backlog/done/` for
settled ones. Item *kind* is carried by a filename prefix (`rfc-` and `bug-`
today; `todo-` if such items are added later), not by a directory. Keeping the
tree shallow is intentional: a file is always one or two levels deep.

An item is an **RFC** when the question is *what the API should be* — the
current behavior is defensible and the proposal argues for changing it. An item
is a **bug** when the current behavior cannot be defended at all under its own
documented contract (input dropped, output wrong, a documented invariant
violated). A bug report is shorter than an RFC: it needs a reproduction, the
observed behavior, and the expected behavior, not a full design rationale.

## Naming

`YYYYMMDD-<kind>-<slug>.md`

- `YYYYMMDD` is the date the item was created, e.g. `20260915`.
- `<kind>` is `rfc` or `bug`.
- `<slug>` is a short lower-case hyphenated summary, e.g. `csi-parser-fallback`.

Examples: `20260915-rfc-push-char-clip-signal.md`,
`20260915-bug-csi-parser-holds-incomplete-sequence.md`

There is intentionally **no sequence number**. Items are not cited by number,
and neither `src/` nor `docs/` may reference them, so a stable identifier buys
nothing. The date prefix sorts the directory chronologically, which makes it
obvious at a glance which items are oldest. Collisions are avoided by the
slug; two items on the same day simply get different slugs. If two items would
share a date and a slug, they are really one item and should be merged.

Because no numbering is used, there is no counter file to maintain.

The templates are named `00000000-rfc-template.md` and
`00000000-bug-template.md`, not `template-rfc.md` and `template-bug.md`, so
that they sort ahead of every real item (digits sort before letters in git's
ordering, so a leading `t` would not). The `00000000` is a date placeholder
that no item will ever use; the `rfc-` and `bug-` kind prefixes keep the
templates consistent with the naming scheme above, and there is one template
per kind because a bug report is shaped differently from an RFC (see the
kind table above).

## States

An item has one of two states, expressed by its location:

| State  | Location          | Meaning                                        |
| ------ | ----------------- | ---------------------------------------------- |
| `open` | `backlog/`        | Being drafted or discussed.                    |
| `done` | `backlog/done/`   | Settled; no longer being worked on.            |

Whether a settled RFC was **accepted** or **rejected** is recorded in its
`Status` field and in an `## Outcome` section, which is written when the
proposal is settled and left out while it is open. It is deliberately not
split into separate directories:

- tuinix does not value long-term stability as a primary goal, so "accepted"
is not a permanent commitment. An accepted RFC may later be changed or even
removed, which makes an "accepted" label on an old RFC misleading rather than
informative.
- `accepted` and `rejected` are the same operational state: the discussion is
over and nobody is working on it. One `done/` directory expresses that; the
qualitative difference belongs in the text, where the reasoning lives.

`postponed` items stay `open` (in `backlog/`), because they are still
unresolved.

A **bug** is settled when it is fixed (moved to `done/` with the fix) or when
it is determined to be intended behavior (moved to `done/` with the reason it
is not a bug).

## Moving an item to `done/`

Use `git mv` so history follows the file:

```sh
git mv backlog/20260915-rfc-push-char-clip-signal.md backlog/done/
```

Update the `Status` field in the same commit.

## Pull requests

An RFC that is implemented reaches `main` through a pull request, like any
other change. The same one-way rule applies to it: a pull request must make
sense to someone who never opens `backlog/`.

Write the title and body as a description of the change itself, not of the
proposal process. Name what the change does and why, the way any other pull
request would. Do not mention that an RFC was accepted or rejected, and do
not cite a path under `backlog/`; a reviewer who wants the reasoning can
find it, and a reader who does not need it is not sent there.

A bug is fixed by an ordinary pull request that reproduces the bug in a test
and fixes it; the bug report itself is not part of the change's public story.

## Packaging

The `backlog/` directory is development material and is excluded from the
published crate. This is configured in `Cargo.toml`:

```toml
exclude = ["backlog/"]
```

Do not remove that line. It keeps items out of the crate tarball without
affecting what git tracks.
