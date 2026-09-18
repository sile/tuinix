---
Created: 2026-09-17
Status: draft
---

# RFC: Let `Style` carry a palette color, not only RGB

## Summary

Add an indexed/palette color representation to tuinix's color type, so a
caller converting a terminal cell that holds a palette index (not an RGB
triple) can carry that index into a frame instead of projecting it to RGB.

## Motivation

A caller that draws the contents of a terminal emulator into a tuinix frame
has two color vocabularies in hand: the cells it is reading, which may hold an
indexed palette color (`0..=255`), and tuinix's `Style`, whose color fields
hold RGB. Converting a cell to a frame therefore cannot preserve the index. The
caller must pick:

- project the index to RGB itself, guessing what the palette's entry 4 is on
  the terminal that will draw the frame, or
- drop the color.

Neither is right. The index is meaningful on the target terminal — that is the
*entire point* of the 256-color mode — and the caller often cannot know the
palette in advance (the user's terminal, or its theme, chooses it). An RGB
projection bakes in one terminal's palette and is wrong on every other.

This is not hypothetical for a layer that copies terminal cells into a frame;
it is the first thing such a renderer hits, and the two options above are the
only ones available today.

## Guide-level explanation

Before:

```rust
// reading a cell whose color is palette entry 4
let style = Style {
    fg: Some(project_palette_to_rgb(4)), // which palette? no good answer
    ..Style::default()
};
```

After:

```rust
let style = Style {
    fg: Some(Color::Indexed(4)),
    ..Style::default()
};
```

The index survives to the frame. A terminal that knows its palette draws it
correctly; a caller that genuinely has RGB keeps using it.

## Reference-level explanation

```rust
/// A color a frame can be drawn in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    /// The terminal's default foreground or background, as appropriate.
    Default,
    /// An entry in the terminal's palette (0..=255).
    Indexed(u8),
    /// A 24-bit color.
    Rgb(u8, u8, u8),
}
```

`Style`'s color fields become `Option<Color>`. The change is in three parts:

- **The type.** Today the fields hold RGB (a tuple or a small struct). Widening
  them to include `Default` and `Indexed` is the whole proposal.
- **`Default`.** `Color::Default` distinguishes "no color chosen" from "white"
  (`Rgb(255, 255, 255)`), which is what `None` does today. The RFC keeps `None`
  as "no color set" and adds `Some(Color::Default)` as "the terminal default",
  or folds the two by making `Color::Default` the `Default` value — an open
  question, because the two answer different questions (unset vs. explicitly
  default).
- **Rendering.** The terminal writer must emit the index ([38;5;Nm) for
  `Indexed` and the RGB sequence ([38;2;r;g;bm) for `Rgb`.
  The writer that backs tuinix today already has both sequences available; the
  work is in the mapping, not in a new escape.

This is deliberately the *smallest* version: three color kinds, no attention to
how a `Default` foreground differs from a `Default` background, no color beyond
the cell. If a project needs named/style colors, that grows the type and should
be its own proposal.

## Alternatives

### Leave the caller to project to RGB

This is the status quo, and it is only acceptable if tuinix treats color as
purely cosmetic. A caller that copies cells does not, so the status quo means
the caller gets colors wrong by construction.

### Add `Indexed` alone, keep the fields non-optional

Rejected as incomplete. The same copying caller also has cells with no color
set, and `None` already models that; removing the option to add indices would
fix one case and lose the other.

### Define a color trait and let callers implement conversion

Rejected as too much surface for the problem. Three concrete kinds cover the
cases; a trait moves the decision to every caller and adds generics to `Style`.

## Drawbacks

- `Style` is the type every drawing call takes by value. Growing it from an RGB
  triple to a tagged enum is more bytes and a branch on every color, in the
  hottest loop in the crate.
- The `None` versus `Color::Default` question above is real and must be settled
  before implementing; guessing wrong either loses "unset" as a concept or
  makes the common case verbose.
- Any palette-index handling is only as good as the terminal it targets.
  tuinix does not read the terminal's palette (it does not own the terminal),
  so an index is carried faithfully and drawn as whatever the terminal says.

## Open questions

- `None` for unset *and* `Color::Default` for the terminal default, or fold the
  two?
- One color type for both foreground and background, or a smaller one if
  background colors stay RGB-only (the frame's own drawing may not need an
  indexed background)?
