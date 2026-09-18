# RFC: Let `Style` carry a palette color, not only RGB

- Status: accepted

## Summary

Add an indexed/palette variant to tuinix's color type, so an application
converting a terminal cell that holds a palette index (not an RGB triple) can
carry that index into a frame instead of projecting it to RGB.

`Color` becomes an enum with two variants, `Indexed(u8)` and `Rgb(u8, u8, u8)`.
The existing named constants (`Color::GREEN` and friends) keep their values;
`Color::new(r, g, b)` becomes `Color::Rgb(r, g, b)`.

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
let style = Style::new().fg_color(Color::GREEN);
```

`Color::GREEN` and the other named constants are unchanged by this proposal.

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
  shapes the terminal protocol has, and `Color::new(r, g, b)` becomes
  `Color::Rgb(r, g, b)`. The named constants keep their values
  (`Color::GREEN` is still `Color::Rgb(0, 255, 0)`), so callers that only name
  a color do not change.
- **Ordering.** `PartialOrd` and `Ord` are dropped. They were derived on the
  struct and compared RGB components in order, which nobody can have meant;
  on an enum they would compare variant tags first. `PartialEq`, `Eq`, and
  `Hash` stay. This is the same reasoning that removed ordering from `Size`.
- **Rendering.** `Style`'s `Display` emits the index ([38;5;Nm) for
  `Indexed` and the RGB sequence ([38;2;r;g;bm) for `Rgb`.
  `Style`'s `FromStr` gains the matching branches so a parsed style round-trips.
  Both are additions to an existing mapping, not a new escape sequence.

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

Rejected for now. `Color::Default` would mean "the terminal's default
foreground or background", distinct from `None` ("no color set at all"). The
two are different questions, but no caller seen so far needs both: each either
names a color or leaves the field unset. The second concept can be added as a
variant later without breaking `Indexed`/`Rgb`. Adding it now would also force
a choice about what `Style::RESET` means, since a fully reset style is the
terminal's default at that moment, which is `None` today.

### Make the fields non-optional

Rejected. `None` already models "the caller set no color", and that is a real
state: a style with no `fg_color` emits no foreground sequence, so whatever the
terminal was already using survives. Dropping `Option` would force every caller
to pick a color and would make `Style::RESET` unable to exist.

### Define a color trait and let callers implement conversion

Rejected as too much surface for the problem. Two concrete kinds cover the
cases; a trait moves the decision to every caller and adds generics to `Style`.

## Drawbacks

- This is a breaking change to `Color`: it becomes an enum, `Color::new()`
  becomes a variant, and the fields stop being `r`, `g`, `b`. Code that reads
  those fields to build its own escape sequence must match on the variants.
- `Color` loses `PartialOrd` and `Ord`, which is breaking on its own for any
  caller that sorted by it.
- The type can no longer be destructured as a plain RGB triple. A caller that
  genuinely has RGB now writes `Color::Rgb(r, g, b)` where it wrote
  `Color { r, g, b }`, and `Color::GREEN` masks the difference only for the
  sixteen named colors.
- An enum costs a tag byte and a match where a three-byte struct did not, on
  the type that every drawing call takes by value and every cell stores.
- Any palette-index handling is only as good as the terminal it targets.
  tuinix does not read the terminal's palette (it does not own the terminal),
  so an index is carried faithfully and drawn as whatever the terminal says.

## Unresolved questions

None; the three points below were settled when this item was decided.

- `Color::Default` is not added. `None` keeps its meaning as "no color set",
  and a distinct "terminal default" variant can be added later.
- `Indexed` covers `0..=255` in one variant. The first sixteen entries are not
  given their own type or constants; they are the same escape sequence and the
  same namespace.
- Foreground and background keep the same type. There is no reason an indexed
  background is less meaningful than an indexed foreground.

## Future possibilities

- A `Default` variant, if a caller ever needs to say "the terminal's default"
  distinctly from "unset".
- Named colors for the first sixteen palette entries, if `Color::Indexed(4)`
  proves too anonymous to read.
- Reading the terminal's palette, which would let tuinix resolve an index to
  RGB. This is a much larger change and tuinix deliberately does not own the
  terminal, so it is not implied by this proposal.
