# Writing into a Frame

This document describes how a [`Frame`](crate::Frame) stores characters: where
a write goes, what happens when it does not fit, and what a position means. For
the byte sequences that produce input, see
[input-decoding](crate::docs::input_decoding).

## Every write names its position

A frame is a grid of styled characters and nothing else. It holds no write
cursor, no "current" position, and no memory of the last write; every method
that stores something takes the [`Position`](crate::Position) it targets.

| Method | Effect |
| ------ | ------ |
| [`put_char()`](crate::Frame::put_char) | Writes one character at a position, and returns the position just past it |
| [`put_frame()`](crate::Frame::put_frame) | Writes another frame as a rectangle whose top-left corner is at a position |
| [`fits()`](crate::Frame::fits) | Reports whether a character written at a position would be drawn |

The position a write lands at is not the terminal's display cursor; that is a
separate thing you pass to [`render()`](crate::Frame::render) when you draw the
frame out.

Because the caller holds the position rather than the frame, a run of writes is
a loop that threads it through:

```rust
# let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
# let text = "abc";
let mut at = tuinix::Position::ORIGIN;
for c in text.chars() {
    let ch = tuinix::Char::new(c, 1, tuinix::Style::new()).expect("valid char");
    at = frame.put_char(at, ch);
}
# assert_eq!(at, tuinix::Position { row: 0, col: 3 });
```

Newlines and tabs are position arithmetic on the same value, not writes:
[`Position::next_line()`](crate::Position::next_line) moves to column 0 of the
next row, and [`Position::next_tab_stop()`](crate::Position::next_tab_stop)
moves to the next multiple of the tab width (at least one full stop). The
columns a tab skips are left blank; nothing is written there explicitly.

To compose a screen out of overlapping pieces, build each layer in its own
frame and paste it with [`put_frame()`](crate::Frame::put_frame), which writes
every cell of the source rectangle — including its blanks — onto the
destination.

## When a character does not fit

[`put_char()`](crate::Frame::put_char) stores a character only when both of
these hold:

- the position is on a row that exists (`at.row < size().rows`), and
- the character ends within the row (`at.col + ch.width() <= size().cols`).

[`fits()`](crate::Frame::fits) is exactly that test, and reports what the write
will do. Two different situations make it `false`: the character would
overflow the right edge of its row, or the position is already below the last
row.

A character that does not fit is dropped, but the position still advances by
its width, so `put_char` returns the same value either way. That is deliberate:
a text-oriented caller keeps a position that describes "after the character I
asked to write", so the arithmetic stays predictable no matter where the frame
ended.

Because the position advances the same way either way, the two causes cannot be
told apart after the fact — the returned position does not say which one
happened. Ask `fits()` before writing instead:

```rust
# let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
# let ch = tuinix::Char::new('a', 1, tuinix::Style::new()).expect("valid char");
# let mut at = tuinix::Position::ORIGIN;
if !frame.fits(at, ch) {
    // the row is full: wrap before writing
    at = at.next_line();
}
at = frame.put_char(at, ch);
```

Choosing what to do about a clip — wrap, pad, or stop — has to happen before
the write anyway, since it decides where the write goes. A position returned
by a "did it fit" flag would arrive after the fact and could only describe a
decision the caller has already had to make.

## How a wide character occupies its cells

A character of width 2 occupies two cells but is stored once, at the column
where it starts. The column to its right belongs to that character: it is not
reported as a position of its own by [`chars()`](crate::Frame::chars), which
yields the character at its starting column and skips over the rest of its
width.

So the cells a frame covers and the positions it reports do not line up one to
one. A row of 4 columns fits `あ あ` as two characters while covering the same 4
cells as `a b c d`, and [`size()`](crate::Frame::size) counts cells, not
characters. This is also why the clipping rule above is stated in cells: a
width-2 character needs two of them, so it is dropped when only one is left in
the row, and none of its cells land.

The pairing assumes the declared width matches what the terminal draws, which
is the caller's declaration to get right — see [`Char`](crate::Char).

## Position arithmetic past the edges

Only the write tests the frame bounds. The [`Position`](crate::Position)
methods do not: [`next_line()`](crate::Position::next_line) and
[`next_tab_stop()`](crate::Position::next_tab_stop) return whatever coordinate
the arithmetic produces, even below the last row or past the last column,
without complaining. A position is a plain coordinate, not a claim about the
frame.

This is what makes the wrap loop above work: `fits()` answers both edges, so
the caller never compares the position against [`size()`](crate::Frame::size)
by hand. A character that does not fit at the start of the next row either ends
the loop, so a frame narrower than a single character cannot spin.

## Overlapping writes

A write at a position replaces what is there, and does so in the same way
whether it arrives through [`put_char()`](crate::Frame::put_char) or
[`put_frame()`](crate::Frame::put_frame):

- a wide character that starts to the left and spans into the target cells is
  removed whole, so no partial glyph is left behind,
- the cells the new character covers are cleared,
- the new character is stored at its starting position.

`put_frame` applies that rule to every cell of the source, including the cells
the source never wrote, which arrive as [`Char::BLANK`](crate::Char::BLANK) and
overwrite whatever is in the destination. So it replaces a rectangle rather
than merging one, and the source's blanks are not transparent. Writing a blank
character over a cell is therefore also how a cell is erased; there is no
separate delete.

The single-cell and rectangle writes differ only at the edges. `put_char`
sweeps nothing and reports the next position, so it is meant to be part of a
run. `put_frame` places a whole rectangle at once and returns nothing, because
a rectangle has no single "position just past it".
