# RFC: Let `Style` carry a palette color, not only RGB

- Status: accepted

## Summary

Make tuinix's color type an enum with an indexed/palette variant, so an
application converting a terminal cell that holds a palette index (not an RGB
triple) can carry that index into a frame instead of projecting it to RGB.

`Color` becomes an enum with two variants, `Indexed(u8)` and `Rgb(u8, u8, u8)`.
The sixteen named constants (`Color::GREEN` and friends) become palette
indices, and `Color::new(r, g, b)` is removed in favor of `Color::Rgb(r, g, b)`.

## Motivation

A caller that copies the cells of a terminal emulator into a tuinix frame has
two color vocabularies in hand: the cells it is reading, which may hold an
indexed palette color (`0..=255`), and `Style`, whose color fields hold `Color`.

Today `Color` is RGB and nothing else, so the index cannot survive the copying.
The caller must pick:

- project the index to RGB itself, guessing what palette entry 4 is on the
  terminal that will draw the frame, or
- drop the color.

Neither is right. The index is meaningful on the target terminal — that is the
*entire point* of the 256-color mode — and the caller often cannot know the
palette in advance (the user's terminal, or its theme, chooses it). An RGB
projection bakes in one terminal's palette and is wrong on every other.

What makes this a gap in tuinix rather than a limitation of the caller is that
there is no way around it from outside the crate. A frame stores a `Style` per
character and `Style` holds `Color`, so there is no place to put an index.
Writing the escape sequence by hand is not an option either: `Frame::put_char()`
takes a `Char`, and a `Char` carries a `Style`.

This matches how tuinix already treats input it does not interpret. The decoder
hands `Input::Unrecognized` bytes and `Input::Paste` bodies to the caller
unmodified rather than guessing at them, precisely because only the caller can
read them. A palette index is the same shape: tuinix cannot resolve it (it does
not own the terminal), so it should carry it rather than collapse it to a guess.

The named constants are in the same position. ANSI assigns the first sixteen
palette entries the color names tuinix already uses, so `Color::RED` naming
`Indexed(1)` is what the name has always meant. Today it names an RGB triple
that tuinix chose, which pins one palette onto every terminal: the value for
`BRIGHT_RED` is `(255, 100, 100)`, an approximation that matches no terminal's
bright red. Naming the palette slot instead lets the terminal draw its own red,
and is the only reading under which the constants are honest.

## Guide-level explanation

Before:

```rust
// copying a cell whose color is palette entry 4
let style = Style::new().fg_color(project_palette_to_rgb(4)); // which palette? no good answer
```

After:

```rust
let style = Style::new().fg_color(Color::Indexed(4));
```

The index survives to the frame. A terminal that knows its palette draws it
correctly; a caller that genuinely has RGB keeps using it:

```rust
let style = Style::new().fg_color(Color::Rgb(0, 255, 0));
```

The named constants name palette slots, so a terminal draws them in its own
colors:

```rust
let style = Style::new().fg_color(Color::GREEN); // palette entry 2
```

## Reference-level explanation

```rust
/// A color a frame can be drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// An entry in the terminal's palette (0..=255).
    Indexed(u8),
    /// A 24-bit color.
    Rgb(u8, u8, u8),
}
```

`Style`'s color fields (`fg_color` / `bg_color`) stay `Option<Color>`. The
change is in three parts:

- **The type.** `Color` grows from a struct to an enum with the same two
  shapes the terminal protocol has, and `Color::new(r, g, b)` is removed
  (`Color::Rgb(r, g, b)` is the constructor). The sixteen named constants
  become palette indices: `Color::BLACK` is `Indexed(0)` through
  `Color::WHITE` at `Indexed(7)`, then `Color::BRIGHT_BLACK` at `Indexed(8)`
  through `Color::BRIGHT_WHITE` at `Indexed(15)`.
- **The constants' documentation.** Each constant's doc used to print an RGB
  triple ("ANSI red color (RGB: 255, 0, 0)"). It now names the palette entry
  ("ANSI red (color index 1)"), which is what the name referred to all along.
- **Ordering.** `PartialOrd` and `Ord` are dropped. They were derived on the
  struct and compared RGB components in order, which nobody can have meant;
  on an enum they would compare variant tags first. `PartialEq`, `Eq`, and
  `Hash` stay. This is the same reasoning that removed ordering from `Size`.
- **Rendering.** `Style`'s `Display` emits the index ([38;5;Nm) for
  `Indexed` and the RGB sequence ([38;2;r;g;bm) for `Rgb`.
  `Style`'s `FromStr` gains the matching branches so a parsed style round-trips.
  Both are additions to an existing mapping, not a new escape sequence.
  The same split applies to the background (`;48`).

This is deliberately the *smallest* version: two color kinds, no `Default`, no
color beyond the cell. If a project needs a "terminal default color" as a
distinct concept, or named theme colors, that grows the type and should be its
own proposal.

## Alternatives

### Leave the caller to project to RGB

This is the status quo, and it is only acceptable if tuinix treats color as
purely cosmetic. A caller that copies cells does not, so the status quo means
the caller gets colors wrong by construction.

### Add a `Default` variant for the terminal's default color

Rejected. `Color::Default` would mean "the terminal's default foreground or
background", which is a different question from `None` ("this style says
nothing about the color"). Both are real, but they are not the same state, and
the second is the one a style needs: a style is a complete replacement, so
being able to leave a field unset is how a caller says "fg is red, leave the
background alone". A variant cannot express that, and adding one alongside
`Option<Color>` would give two ways to say something close to the same thing.
It can also be added later without breaking `Indexed`/`Rgb`, so there is no
reason to guess now. See "Make the fields non-optional" for why `Option`
stays.

### Make the fields non-optional

Rejected, for the reason above and because it would break `Style::RESET`.
`None` models "the caller set no color", which is what `RESET` means: it emits
no color sequence and lets whatever the terminal is already using survive.
Dropping `Option` would force every caller to name a color, and would make
`RESET` mean "command the terminal's defaults back" -- a different and heavier
statement than "say nothing about color". It would also make
`Style::new().fg_color(Color::RED)` reset the background as a side effect.

### Define a color trait and let callers implement conversion

Rejected as too much surface for the problem. Two concrete kinds cover the
cases; a trait moves the decision to every caller and adds generics to `Style`.

## Drawbacks

- This is a breaking change to `Color`: it becomes an enum, `Color::new()`
  is gone (write `Color::Rgb(r, g, b)` instead), and the fields stop being
  `r`, `g`, `b`. Code that reads those fields to build its own escape sequence
  must match on the variants.
- The named constants change meaning. `Color::RED` stops being the RGB triple
  `(255, 0, 0)` and becomes palette entry 1, so it draws whatever the terminal
  calls red. Colors that were already written with the constants follow the
  terminal from now on, and `Color::WHITE` (entry 7) no longer coincides with
  `Color::BRIGHT_WHITE` (entry 15) on terminals that distinguish them.
- `Color` loses `PartialOrd` and `Ord`, which is breaking on its own for any
  caller that sorted by it.
- The type can no longer be destructured as a plain RGB triple. A caller that
  genuinely has RGB now writes `Color::Rgb(r, g, b)` where it wrote
  `Color { r, g, b }`.
- An enum costs a tag byte and a match where a three-byte struct did not, on
  the type that every drawing call takes by value and every cell stores.
- Any palette-index handling is only as good as the terminal it targets.
  tuinix does not read the terminal's palette (it does not own the terminal),
  so an index is carried faithfully and drawn as whatever the terminal says.
  This applies to the named constants as much as to a caller's own index.

## Unresolved questions

None; the three points below were settled when this item was decided.

- `Color::Default` is not added. `None` keeps its meaning as "no color set",
  and a distinct "terminal default" variant can be added later.
- `Indexed` covers `0..=255` in one variant. The first sixteen entries keep
  their names as the existing constants, which are now `Indexed`; they get no
  separate type and share the escape sequence and namespace with the rest.
- Foreground and background keep the same type. There is no reason an indexed
  background is less meaningful than an indexed foreground.

## Future possibilities

- A `Default` variant, if a caller ever needs to say "the terminal's default"
  distinctly from "unset".
- Named constants for the rest of the palette (entries 16..=255), if
  `Color::Indexed(4)` proves too anonymous in a place where a constant cannot
  reach.
- Reading the terminal's palette, which would let tuinix resolve an index to
  RGB. This is a much larger change and tuinix deliberately does not own the
  terminal, so it is not implied by this proposal.
