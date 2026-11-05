# RFC: Ask the terminal for bracketed paste

- Status: accepted

## Summary

Give `TerminalDriver` the terminal mode that makes a paste arrive as a paste:
`enable_bracketed_paste()` and `disable_bracketed_paste()`, with the driver
putting the terminal back into the default mode when it is dropped, beside
the mouse reporting and the alternate screen it already takes back. The
decoder side already exists -- `Input::Paste` is parsed today -- so this is
the missing half that makes the decoder ever see one.

## Motivation

`InputDecoder` understands `ESC [ 200 ~ ... ESC [ 201 ~` and reports the body
as [`Input::Paste`]. It never receives those markers in practice. A terminal
sends them only to an application that has put it in bracketed-paste mode with
`ESC [ ? 2004 h`; without that request, a paste is a burst of ordinary bytes
and is decoded as the keys it looks like. A newline inside the pasted text
becomes `Enter`, a `C-c` inside it becomes `C-c`, and a paste of a config file
becomes an editing session that ran commands.

That leaves the input side of the crate unable to describe an event its own
`Input` enum has a variant for. A consumer reading the `Input` documentation
sees a paste variant, enables nothing, and never gets one; the only way to
receive the variant is to send a raw escape sequence to the terminal, which is
exactly the byte-level work the driver exists to keep out of an application.

The driver already owns every other mode of the same kind. It enters and
leaves raw mode and the alternate screen, and it turns mouse reporting on and
back off, all of them terminal modes the application asks for by name and
never spells out. Bracketed paste is the same kind of mode and is the one that
is missing.

There is a correctness cost to leaving it out, not only a convenience one. A
mode an application sets itself has to be unset on every way out, and the
paths out of a terminal program are early returns, panics, and process exit.
Today each application that wants bracketed paste has to reimplement the
driver's own cleanup discipline for the one sequence the driver does not
manage, and any path it forgets leaves the user's shell in a mode it did not
ask for.

## Guide-level explanation

The decoder has described a paste for as long as the variant has existed; the
driver has simply never asked the terminal to make one possible. The change is
to make the driver's modes complete:

```rust
// Before: a paste arrives as the keys it looks like, and input.rs's
// `Input::Paste` is unreachable.
fn main() -> io::Result<()> {
    let mut driver = TerminalDriver::new()?;
    driver.enable_mouse_reporting()?;
    // ...
}

// After: the terminal brackets a paste, so the decoder can report it.
fn main() -> io::Result<()> {
    let mut driver = TerminalDriver::new()?;
    driver.enable_mouse_reporting()?;
    driver.enable_bracketed_paste()?;
    let mut decoder = InputDecoder::new();
    // ... a paste now arrives as `Input::Paste { bytes }`.
    Ok(())
}
```

An application that does not call `enable_bracketed_paste()` sees exactly what
it saw before: the sequence is a request, and a terminal that never received
it never brackets anything. Enabling the mode is therefore not a breaking
change for existing applications, and the state the mode adds is opt-in the
same way mouse reporting is.

Whoever turns it on does not have to turn it off. Dropping the driver writes
`ESC [ ? 2004 l` before the terminal goes back to its normal screen, the same
way it already takes back the mouse and raw mode. An application that exits
early still leaves the terminal as it found it, because the driver is dropped
on those paths too.

A terminal that does not know the mode ignores it, which is what a terminal
that does not understand a mode does with any of them. The call then succeeds
and pastes keep arriving as ordinary input, so the mode is a request that a
terminal may decline, not a capability the call can check.

## Reference-level explanation

### API

Two methods on `TerminalDriver`, beside `enable_mouse_reporting()` and
`disable_mouse_reporting()`:

```rust
impl TerminalDriver {
    /// Puts the terminal in bracketed-paste mode.
    pub fn enable_bracketed_paste(&mut self) -> io::Result<()>;

    /// Takes the terminal back out of bracketed-paste mode.
    pub fn disable_bracketed_paste(&mut self) -> io::Result<()>;
}
```

Both write their sequence to the driver's buffered output and flush, exactly
as the mouse methods do, so a caller that has just called one can hand the
return value straight out of its own setup, and so the sequence is not left
sitting in the buffer behind an event loop that blocks on input.

### Bytes

- Enable: `ESC [ ? 2004 h` -- `b"\x1b[?2004h"`.
- Disable: `ESC [ ? 2004 l` -- `b"\x1b[?2004l"`.

These are the DEC private mode pair xterm introduced and that terminals
implementing `Input::Paste` must speak. There is no variant of the sequence
worth choosing between: the mode number and both spellings of the terminator
are fixed by the terminals, and the decoder side of the crate already parses
the `200` / `201` pair that this request produces.

### `Drop`

`Drop for TerminalDriver` gains one line, in the same shape as the ones
already there:

```rust
fn drop(&mut self) {
    let _ = self.disable_bracketed_paste();
    let _ = self.disable_mouse_reporting();
    let _ = self.disable_alternate_screen();
    let _ = self.disable_raw_mode();
    let _ = self.show_cursor();
    let _ = self.output.flush();
    // ...
}
```

It goes before the alternate screen is left, beside the mouse reset, so the
mode resets stay contiguous and ahead of the screen and raw-mode teardown.
Ordering against the other modes does not matter to a terminal; the position
is chosen to keep the block readable, not to satisfy one.

Unlike the mouse, which is enabled and disabled as one block by two methods
with no way for the driver to know whether a caller enabled it, disabling
bracketed paste unconditionally in `Drop` is safe: writing the disable
sequence for a mode that was never enabled is the same no-op the terminal
treats it as for the mouse, whose disable sequence is also written whether or
not it was enabled.

### Interaction with `Input::Paste`

Nothing in the decoder changes. `Input::Paste` is already produced for a
complete `ESC [ 200 ~ ... ESC [ 201 ~`, and this RFC is what makes a terminal
send that pair. A consumer that enables the mode and inserts the body gets a
paste that arrives as one value; one that enables it and ignores the variant
implements the variant's own contract (the decoder reports, the application
decides).

### What the encoder cannot do

A paste *body* may contain any bytes, including the `ESC [ 201 ~` that closes
it; the decoder handles that by finding the first close marker, which is what
the terminal guarantees by sending one that the pasted text cannot contain.
This RFC adds no encoding side and does not change that rule.

## Drawbacks

- One more pair of methods on a type that already carries several modes, and
  one more line in `Drop`. The type's job is exactly this, but each mode added
  makes `TerminalDriver` a little more of a grab bag of terminal state.
- Enabling the mode changes what an application receives: on a terminal that
  supports it, a paste that used to arrive as keys now arrives as one
  `Input::Paste`. An application that enabled the mode and only handles keys
  would see a paste go from "many key events" to "one event it ignores".
  That is why enabling stays explicit and is not done by `TerminalDriver::new()`.

## Rationale and alternatives

- **Do nothing.** The decoder variant stays unreachable through the driver.
  Every application that wants it writes the sequence itself and reimplements
  the cleanup, and the one that forgets leaves the shell in a mode with no
  visible cause.
- **Enable it in `TerminalDriver::new()`.** The mode would then be on for every
  application that constructs a driver, including ones that never look at
  `Input::Paste` and would rather keep pastes as keys. The driver's other
  opt-in modes are methods, not constructor side effects, and this one follows
  them.
- **A single method taking a `bool`, `set_bracketed_paste(true)`.** Reads worse
  at the call site than the named pair, and departs from the mouse methods it
  sits beside. A pair also gives `Drop` something to call that is named after
  what it does.
- **A general `set_private_mode(mode: u16, enabled: bool)`.** Pushes mode
  numbers into the public API and makes every caller pass one, for a set of
  modes whose names the driver exists to keep away from the application.
- **Expose the raw bytes and let callers write them.** The driver already
  implements `Write`, so an application *can* write the sequence today. What
  it cannot do that way is have the driver take the mode back on drop, and
  writing the same two sequences in every application is the duplication this
  removes.

## Outcome

Settled before implementation with no change to the design above. Two
questions the draft carried were closed on the way:

- The methods return `io::Result<()>` and report the write, matching
  `enable_mouse_reporting()` and `disable_mouse_reporting()`. A caller that
  treats a mouse-reporting failure as a warning treats this one the same way;
  no caller wanted it silent.
- Nothing queries whether the terminal accepted the mode, because a terminal
  cannot answer it. The call reports the write, not the effect.

The scope is unchanged from what is described above.

## Unresolved questions

None. The two questions carried by the draft were settled as described in
`## Outcome`.

## Future possibilities

- The same shape would fit any further opt-in input mode: a named pair, and a
  line in `Drop`.
- If the crate ever grows a way to describe its terminal modes as data rather
  than as methods, the pair becomes one row in that description and `Drop`
  walks the rows instead of naming each one.
