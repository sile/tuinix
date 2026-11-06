# RFC: Make bracketed paste opt-in again

- Status: accepted

## Summary

Stop turning bracketed-paste mode on in `TerminalDriver::new()` and
`TerminalDriver::with_input()`, and give the driver the pair
`enable_bracketed_paste()` / `disable_bracketed_paste()` that the previous RFC
considered and set aside. The mode is then a method an application calls,
beside `enable_mouse_reporting()` and `disable_mouse_reporting()`, and the
driver takes it back on drop for the applications that asked for it.

## Motivation

Since `0.8.0` the driver asks the terminal for bracketed paste whenever it is
constructed. On Windows terminals -- WSL2 in particular -- pasting into such an
application loses the newlines: a multi-line paste arrives as one
[`Input::Paste`](crate::Input::Paste) whose body has the line breaks removed, so
a pasted config file becomes one line. The same paste on macOS arrives intact.
The failure is in the terminal's handling of the mode rather than in the
decoder, which reports the body exactly as the terminal sent it, but the effect
is that the mode the driver turned on by itself makes paste worse for every
application running under those terminals.

Pastes worked before `0.8.0` on those terminals because no application asked
for the mode unless it meant to. That is the argument the previous RFC
dismissed: it enabled the mode by default on the grounds that it could only
make a paste safer, comparing it to raw mode and the alternate screen, which
the driver also turns on unasked. Those modes are not the same kind of thing.
Raw mode and the alternate screen are what an application needs in order to
paint and read at all, and a terminal cannot break them without breaking every
other program on it. Bracketed paste is a mode whose only effect is to change
what the terminal sends back, and a terminal that implements it badly degrades
the input an application receives rather than failing visibly. A default that
is only safe when the terminal implements it correctly is not a safe default.

## Guide-level explanation

The decoder has always understood the markers, whether or not anything asked
for them. What changes is who asks:

```rust
// 0.8.0: the driver asks for the mode itself; there is no way to decline it.
let mut driver = TerminalDriver::new()?;

// After: the driver leaves the mode alone unless the application asks.
let mut driver = TerminalDriver::new()?;
if pastes_are_wanted {
    driver.enable_bracketed_paste()?;
}
```

An application that does not enable the mode sees pastes as it did before the
mode existed: a burst of ordinary bytes decoded as the keys they look like.
That is the behaviour that worked on every terminal, including the ones where
the mode is broken, so it is the behaviour an application gets without saying
anything.

An application that enables the mode gets the guarantee the mode exists for: a
paste arrives as one [`Input::Paste`](crate::Input::Paste), and a newline in
the pasted text is text rather than `Enter`. Turning it on is a decision the
application makes per terminal population it expects to run under, which is
exactly the kind of decision the driver leaves to the caller after this change.

Calling `disable_bracketed_paste()` is not required: dropping the driver writes
the disable sequence the same way it writes the mouse, alternate-screen, and
raw-mode resets. It exists for the application that turns the mode on for part
of its run and off again, and it is what keeps the pair symmetric with the
mouse methods it sits beside.

There is no check of whether the terminal accepted the mode. A terminal that
does not know `ESC [ ? 2004 h` ignores it, and the application's paste handling
simply never sees a paste -- which is the same situation as an application
running under a terminal that does not implement the mode at all.

## Reference-level explanation

### API

Two methods on `TerminalDriver`, beside the mouse pair:

```rust
impl TerminalDriver {
    /// Puts the terminal in bracketed-paste mode.
    pub fn enable_bracketed_paste(&mut self) -> io::Result<()>;

    /// Takes the terminal back out of bracketed-paste mode.
    pub fn disable_bracketed_paste(&mut self) -> io::Result<()>;
}
```

Both are private today and both become public; the bodies do not change. The
pair is what makes the mode the application's decision in both directions, and
it matches the mouse methods it sits beside. They write their sequence through `write_bracketed_paste_mode()` to the driver's
buffered output and flush, exactly as the mouse methods do, so an application
that has just called one can hand the return value out of its own setup.

### What the constructors do

`TerminalDriver::install()` loses the `enable_bracketed_paste()` call between
`hide_cursor()` and the flush. That is the whole behavioural change on the
output side: no bracketed-paste sequence is written unless the application
calls the enable method.

The `new()` rustdoc drops bracketed paste from the list of modes it turns on
and from the explanation of what [`Input::Paste`](crate::Input::Paste) means to
an application driving a terminal. The sentence that remains in its place says
that the mode is the application's to request, with a pointer to the enable
method.

### `Drop`

`Drop` keeps its unconditional
`let _ = self.disable_bracketed_paste();` line, beside the mouse reset and
ahead of the alternate-screen reset. Writing the disable sequence for a mode
that was never enabled is a no-op to the terminal, and it costs one write for
the applications that never called the enable method. Keeping the line means
the driver never has to remember whether it enabled the mode, which is the same
choice the mouse reset already makes.

### Panic hook

The hook installed by `install()` currently writes `ESC [ ? 2004 l` together
with the alternate-screen reset:

```rust
let _ = write!(stdout, "\x1b[?2004l\x1b[?1049l");
```

That stays. The hook cannot ask the driver whether the mode is on, and a
spurious disable is again a no-op. Dropping the sequence would make the hook
depend on state the driver would then have to carry into a `'static` closure,
for no benefit to the terminals that do have the mode on.

### Tests

The existing tests cover `write_bracketed_paste_mode()`'s bytes and the
decoder's side, and neither changes. What is missing is a test that the
constructor writes no bracketed-paste sequence at all when nothing calls the
method. The driver cannot be built against a `Vec<u8>` -- its output is stdout
-- so the check belongs to whatever the crate already uses to observe
driver-level behaviour; if there is nothing that can observe this, the
absence is recorded here rather than papered over with a test that asserts
nothing about the constructor.

## Drawbacks

- [`Input::Paste`](crate::Input::Paste) becomes unreachable through the driver
  for an application that never calls the enable method, which is the state the
  previous RFC called a defect: the `Input` enum describes an event the driver
  will not produce unless asked. The answer is that a paste arriving as keys is
  a perfectly good reading of the same bytes, and the decoder still produces
  `Input::Paste` for an application that feeds it those bytes from anywhere
  else.
- An application that wants the mode has to call a method it did not have to
  call in `0.8.0`. That is a source change for every such caller, and it is the
  point: the decision moves back to the application.

## Rationale and alternatives

- **Leave it on by default and document the Windows behaviour.** A library
  cannot document a terminal's bug away. The applications running under those
  terminals are worse off after the change than before it, and they have no
  way to get the old behaviour back.
- **Detect WSL2 and enable the mode only elsewhere.** It turns a clear rule
  ("the mode is the application's to request") into a guess about which
  terminals are broken today, made by reading environment variables that do not
  say how a terminal behaves. Terminals other than WSL2's may have the same
  defect, and WSL2's may be fixed later.
- **Enable by default and add a `disable_bracketed_paste()` escape hatch.** A
  default that applications have to opt out of to get correct input is the
  problem in the first place, and it leaves two populations behaving
  differently for the same program.
- **Expose a `set_bracketed_paste(bool)`.** The named pair matches the mouse
  methods beside it and reads better at the call site; a `bool` argument is a
  call site where `true` and `false` mean nothing without the signature.
- **Keep the pair private and turn the mode on inside some other public call.**
  No other call is the right place: every mode the driver turns on unasked is
  one an application needs in order to draw or read at all, and this is not
  one of them.

## Compatibility

No signature is removed and no type gains a variant, so the change is
compatible with the published surface. It is not compatible in behaviour: an
application built against `0.8.0` that relied on the constructor to put the
terminal in the mode now receives pastes as the keys they look like. Such an
application compiles unchanged and quietly loses the mode.

## Unresolved questions

None.

## Future possibilities

- If the Windows terminal's handling of the mode is fixed, or the cause is
  narrowed to something the driver can work around on the output side, an
  opt-in mode is still the right shape: only the recommendation of when to call
  it changes.
- A terminal-mode description as data rather than as methods would let an
  application name the set of modes it wants at construction and leave the
  opt-in question to that value.
