# Writing into a Frame

This document describes how a [`Frame`](crate::Frame) stores characters: where
a write goes, what happens when it does not fit, and what the write position
means. For the byte sequences that produce input, see
[input-decoding](crate::docs::input_decoding).

## The write position

A frame is a grid of styled characters plus a single write position, returned
by [`next_position()`](crate::Frame::next_position). It is not the terminal's
display cursor; that is a separate thing you pass to
[`render()`](crate::Frame::render) when you draw the frame out.

The position starts at the top-left cell and moves only forward:

| Method | Effect on the write position |
| ------ | ---------------------------- |
| [`push_char()`](crate::Frame::push_char) | Advances by the character's width, whether or not the character was stored |
| [`push_newline()`](crate::Frame::push_newline) | Moves to column 0 of the next row |
| [`push_tab()`](crate::Frame::push_tab) | Moves to the next multiple of `tab_width`, at least one full stop |

There is no request for a position, and no way to move backward. A frame is
written the way a terminal paints: left to right, top to bottom, once. To
compose a screen out of overlapping pieces, the usual route is to build each
layer in its own frame and paste it with [`draw()`](crate::Frame::draw), which
writes every cell of the source rectangle — including its blanks — onto the
destination.

## When a character does not fit

[`push_char()`](crate::Frame::push_char) stores a character only when both of
these hold:

- the write position is on a row that exists (`next_position().row <
size().rows`), and
- the character ends within the row (`next_position().col + ch.width() <=
size().cols`).

Otherwise the character is dropped, and the method returns `false`. Two
different situations produce that one result: the character would have
overflowed the right edge of its row, or the write position was already below
the last row.

In both cases the position still advances by the character's width. That is
deliberate: a text-oriented caller that simply keeps pushing characters ends up
with a position that keeps describing "after the character I asked to write",
so the arithmetic stays predictable no matter where the frame ended.

Because the position advances the same way either way, the two causes cannot be
told apart after the fact — `push_char` returning `false` does not say which
one happened. Decide before pushing, from the same values the check above uses:

```rust
# let mut frame = tuinix::Frame::new(tuinix::Size { rows: 2, cols: 4 });
# let ch = tuinix::Char::new('a', 1, tuinix::Style::new()).expect("valid char");
let pos = frame.next_position();
let size = frame.size();
if pos.col + ch.width() > size.cols {
    // the row is full: wrap before writing
    frame.push_newline();
}
if pos.row < size.rows {
    frame.push_char(ch);
}
```

The check is a few expressions against public values, so it is available
today. Asking `push_char` for the reason instead would not remove it either:
choosing what to do about a clip — wrap, pad, or stop — has to happen before
the write, and a returned reason arrives after the position has already moved.

## Moving past the edges is normal

Only [`push_char()`](crate::Frame::push_char) tests the frame bounds.
[`push_newline()`](crate::Frame::push_newline) and
[`push_tab()`](crate::Frame::push_tab) move the position wherever it lands,
even below the last row or past the last column, without complaining. This is
what makes the three methods composable: the position is a plain coordinate
that the caller keeps track of, and the frame reports whether a character made
it in, not whether the coordinate was sensible.

A caller that walks a whole screen therefore ends by comparing the position
against [`size()`](crate::Frame::size) itself, rather than expecting a move to
fail.

## Relationship to other writes

The layer-composition case above is why [`draw()`](crate::Frame::draw) exists:
it is a random-access write of a whole rectangle, and it already handles what a
single-cell version would have to — pasting blanks over what was there,
ignoring cells outside the destination, and removing a wide character that a
later write cut into.

This document describes the append-only cursor, so everything above holds for
the [`push_*`](crate::Frame::push_char) methods. Adding a single-cell write at
an explicit position would not change any of it, but it would raise a new
question that does not exist yet: whether such a write moves the write
position. As long as the position only moves forward through `push_*`, the
prediction rule above is complete; a position-addressed write has to say what
it leaves behind before that stays true.
