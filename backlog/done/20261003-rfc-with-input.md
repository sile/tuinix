# RFC: `TerminalDriver::with_input`

- Status: accepted

## Summary

Add a constructor to `TerminalDriver` that takes its input from a caller-provided
`File` instead of standard input, so that an application can consume standard
input as *data* -- `cat file | mytui` -- while still reading the keyboard from
the terminal it is running on. `stdin.is_terminal()` is `false` in that
situation, and [`TerminalDriver::new()`] rejects it today, so the pattern is not
expressible at all.

The new constructor is `with_input(File)`: the caller opens the terminal it
wants (typically `/dev/tty`, the controlling terminal) and hands it over. It is
the only public addition. There is deliberately no `from_*` convenience
constructor that opens `/dev/tty` itself: the API stays at its minimum, and the
one line of `File::open("/dev/tty")` it would save belongs to the caller. A path
other than `/dev/tty` -- a pty slave in a test, or an explicitly chosen terminal
-- is expressible through the same constructor, so no separate path-shaped form
is needed either.

## Motivation

`TerminalDriver::new()` requires standard input to be a terminal:

```rust
if !stdin.is_terminal() {
    return Err(Error::other("STDIN is not a terminal"));
}
```

and it derives its input descriptor from stdin: it calls `tcgetattr` on fd 0 and
opens a fresh, non-blocking description of the device `ttyname_r(fd 0)` names.
The contract is stated in the type's own documentation -- the driver "owns the
file descriptors for input and output" of the terminal *stdin is connected to*.

The first consumer that needs the other arrangement is a text editor, `kk`. It
wants to accept a pipe the way `less` does, so that `cat file | kk` shows the
file and lets the user move around it, with the keyboard still working. In that
invocation stdin is a pipe, not a terminal, so `TerminalDriver::new()` fails
before anything is drawn. The only descriptor that can deliver keys is the
controlling terminal, and the driver has no way to be told to use it.

Note that "the terminal stdin is connected to" and "the controlling terminal"
are the same device in the ordinary interactive case, but they are not the same
statement, and they can differ:

- `mytui < /dev/pts/5` points stdin at *another* terminal while the controlling
  terminal stays where the process was launched. `new()` reads `/dev/pts/5`;
  opening `/dev/tty` would read a different device.
- A process started with `setsid` has no controlling terminal at all, so
  `/dev/tty` cannot be opened -- even though stdin may still be a terminal.
- A test harness that `openpty`s a pair and hands the slave to a child, or a
  multiplexer (`tmux`, `screen`, `expect`, `socat`) that allocates a pty for its
  child, puts a terminal on stdin that is not necessarily the child's
  controlling terminal.

So the API must let the caller name the input source. `new()`'s contract -- "I
own the descriptors of the terminal **stdin** is connected to" -- must not be
generalized to "the controlling terminal", because that would change what `new()`
reads in exactly these cases. `new()` is left as it is; the new constructor takes
the already-opened input as an argument.

This is not specific to `kk`. Any pager, viewer, or editor that reads a stream
from stdin has the same shape: the data comes in on fd 0 and the keyboard lives
on another descriptor. The knowledge of how to set up a terminal once opened --
`tcgetattr`, raw mode, the SIGWINCH handler, the `OCL` details -- already lives in
`TerminalDriver`; what is missing is a constructor that accepts an input the
caller opened.

The alternative -- every such tool re-implementing a second terminal setup
beside the driver it already uses, with its own `unsafe` `dup2`/`ttyname_r` code
-- is exactly the duplication `TerminalDriver` exists to prevent. The one thing
the caller should own is the choice of *which* terminal (a `File::open`);
everything else stays in the driver.

## Guide-level explanation

An application that reads data from stdin takes its keyboard from the
controlling terminal instead:

```rust
use std::io::{IsTerminal, Read};

fn main() -> std::io::Result<()> {
    // Is stdin a pipe (or a file, or anything that is not a terminal)?
    if std::io::stdin().is_terminal() {
        // The usual case: stdin is the keyboard, so nothing changes.
        let mut driver = tuinix::TerminalDriver::new()?;
        run(&mut driver, String::new())?;
    } else {
        // stdin is data. Read it all, then take the keyboard from the
        // controlling terminal.
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;

        let input = std::fs::File::open("/dev/tty")?;
        let mut driver = tuinix::TerminalDriver::with_input(input)?;
        run(&mut driver, text)?;
    }
    Ok(())
}

fn run(driver: &mut tuinix::TerminalDriver, text: String) -> std::io::Result<()> {
    // ...the same event loop as before; nothing about reading input,
    // resizing, or rendering changes...
    let _ = (driver, text);
    Ok(())
}
```

The application makes one decision (`stdin.is_terminal()`), and uses one
constructor or the other. Everything downstream -- `input_fd()` for `poll`,
`InputDecoder`, raw mode, the resize signal, dropping the driver to restore the
terminal -- is unchanged. The application does not `dup2`, and it does not
re-implement terminal setup; the only thing it owns is the one `File::open`
that names *which* terminal the keyboard is on.

When stdin *is* a terminal, the caller can pass the same terminal `new()` would
have used (open `/dev/tty`, or reuse stdin's device) and get the same input; the
two constructors differ only in who decides the input source. `new()` says
"stdin is my keyboard"; `with_input(file)` says "*this* is my keyboard, whatever
stdin is". Because `with_input` does not look at stdin at all, it works equally
for a pty slave, a re-pointed terminal, or `/dev/tty`.

## Reference-level explanation

One constructor is added to the public API:

```rust
impl TerminalDriver {
    /// Creates a terminal driver that reads input from `input`.
    ///
    /// Unlike [`TerminalDriver::new()`], this does not require standard input to
    /// be a terminal. `input` is used as the driver's keyboard, so an
    /// application that consumes standard input as data (`cat f | mytui`) can
    /// still read keys -- typically by passing a handle to the controlling
    /// terminal (`/dev/tty`). Output is standard output, as with `new()`.
    ///
    /// `input` must be an open, readable terminal device. Raw mode is set on
    /// it, and dropping the driver restores it.
    ///
    /// # Errors
    ///
    /// Returns an error if `input` is not a terminal, if standard output is
    /// not a terminal, if another driver instance already exists, or if a
    /// terminal configuration call fails.
    pub fn with_input(input: File) -> io::Result<Self>;
}
```

The caller owns opening `input`. To use the controlling terminal, that is one
line:

```rust
let input = File::open("/dev/tty")?;
let driver = TerminalDriver::with_input(input)?;
```

Opening `/dev/tty` for input is a plain read-only open (`O_RDONLY | O_CLOEXEC`).
`O_NOCTTY` is not needed: this is a read-only open of the controlling terminal,
and a read-only open cannot acquire a controlling terminal in any case.

`with_input` is built on the shared per-descriptor setup that `new()` also uses
(see below), so there is no separate private helper to promote -- the public
constructor *is* the general form.

### What stays required

The output side is untouched. Both constructors still require `stdout` to be a
terminal and still write to it, because a TUI draws to the terminal it is
attached to. Consequently `cat f | kk > out` and `cat f | kk | pipe` remain
unsupported: redirection or piping of stdout is out of scope here, and is a
separate question (see "Future possibilities").

Because `with_input` takes an already-open `File`, it needs no controlling
terminal of its own: whatever the caller passes is the keyboard. A caller that
reaches for `/dev/tty` still fails when there is no controlling terminal (a
process started with `setsid`, or one whose session leader has exited) -- but
that failure is the caller's `File::open`, not the driver's, and the caller can
choose a different terminal instead. There is no silent fallback to stdin:
falling back would put the keyboard and the data on the same descriptor.

### `new()` is unchanged

The two constructors differ in where the input comes from, and that difference
is the whole point:

- `new()`'s input is the terminal **stdin is connected to**. It derives the
  device from fd 0 (`ttyname_r(fd 0)`), and it does not open `/dev/tty`.
- `with_input(file)`'s input is exactly `file`.

These agree for the ordinary interactive invocation but diverge for
`mytui < /dev/pts/5`, under `setsid`, and for a pty handed to a child. So
`new()` keeps its documented contract -- "I own the descriptors of the terminal
stdin is connected to" -- and is **not** re-expressed as "the controlling
terminal". Changing that would change what `new()` reads in those cases. Only
the per-descriptor mechanics (below) are shared; the choice of input descriptor
is not.

### How the input descriptor is threaded through

`new()` and `with_input()` share a helper that takes the input descriptor; the
pieces `new()` currently hard-codes per descriptor move there:

- `tcgetattr` for the saved termios is taken on the input descriptor, not on fd
  0.
- `ttyname_r` for the panic hook's restore path is taken on the input
  descriptor. The path it remembers is only used to restore the terminal mode
  from the panic hook, after the driver has already opened and configured the
  device, so recording the input descriptor's path is correct in both
  constructors.
- `enable_raw_mode` / `disable_raw_mode` call `tcsetattr` on the input
  descriptor, as they do today (`self.input_fd()`).
- The size query (`query_terminal_size`) is unchanged: it `ioctl`s the *output*
  descriptor, which is stdout in both cases. A `/dev/tty` opened for input is
  the same terminal as stdout in the intended use (the process is attached to
  it), and when it is not, the terminal the user sees is the one being measured,
  which is the right one to measure.

`new()` keeps its documented "stdin or stdout is not a terminal" errors.
`with_input()` replaces the stdin check with a check on the supplied `File`
(it must be a terminal), and needs no "controlling terminal missing" error
because it never opens one itself.

### Interaction with the singleton and the panic hook

The `TERMINAL_EXISTS` singleton already prevents a second driver from being
created, and that is unchanged: the new constructors acquire the same guard, so
their failure modes are "another driver exists", not a second raw-mode entry.
The panic hook installed by the driver is also unchanged -- it restores the mode
on the recorded tty path, and that path now comes from the input descriptor.

### Why not `dup2` in the consumer

A consumer *can* make the existing `new()` work today by reading stdin to the
end and then `dup2`-ing `/dev/tty` over fd 0. That was the alternative the first
consumer considered. It is rejected here because the `unsafe` `dup2` and the
reasoning about when it is safe (the singleton, the panic hook, the ordering
between reading stdin and replacing fd 0) are terminal-setup knowledge that
belongs in the driver, next to what it already shares -- not in each consumer
that wants a pager, and not in a pattern that mutates fd 0 for the rest of the
process. `with_input` reaches the same terminal without touching fd 0.

## Drawbacks

- A new public constructor on the crate's central type. It is additive, but it
  is the first constructor that does not derive its input from stdin, so callers
  must now think about *which* terminal they are reading from rather than
  getting stdin implicitly. The output-side requirement is unchanged, so the
  feature is partial: piping stdout still fails, and a reader may expect it to
  work once stdin piping does.
- The caller now writes `File::open("/dev/tty")`, which is a Unix spelling of
  "the controlling terminal". tuinix is Unix-only already (it links `libc` and
  uses `ttyname_r`, `tcsetattr`, and friends), so this adds no new portability
  constraint, but it makes the controlling terminal an explicit concept at the
  call site where before it was implicit in stdin.

## Rationale and alternatives

- **A `from_*` convenience constructor that opens `/dev/tty` itself
  (`from_controlling_terminal()`, `from_tty()`, `from_dev_tty()`).** Rejected.
  It would be one public API for one line of `File::open`, and it would fix the
  input source to `/dev/tty` -- locking out the pty-slave and re-pointed-terminal
  cases that `with_input` covers. It is also the wrong shape: `from_*` names a
  conversion from a value, but there is no input value to convert from, only a
  path the constructor would open internally. `with_input(File)` keeps the
  knowledge of *which* terminal at the call site, where the caller already
  knows, and adds no API surface that could not be added later anyway.
- **A path parameter on the constructor (`with_input_path(impl AsRef<Path>)`).**
  With `with_input("/dev/tty")` the "with" reads as passing an input value, but
  the constructor would open it internally -- an easy way to write
  `with_input("file.txt")` and get a confusing "not a terminal" error. Taking
  the already-opened `File` makes "is it a terminal?" the constructor's check
  instead of the caller's guess, and keeps the fallible `open` (and its
  `ENXIO`/`ENOENT`) at the call site.
- **Add an input-descriptor setter, or a `Builder`.** A driver that can be
  re-pointed after construction means raw mode, the saved termios, the resize
  signal, and the singleton all need to be re-established mid-life, and an
  application could put them in an inconsistent state. A constructor cannot be
  called at the wrong time. Keeping construction fixed also matches the current
  `new()`.
- **`dup2` `/dev/tty` over fd 0 inside `new()` automatically when stdin is not
  a terminal.** Rejected: it would silently change what fd 0 means for the rest
  of the process (anything the application later does with stdin now reads the
  keyboard), and it would make `new()` succeed in an invocation whose stdin it
  never looked at. Whether stdin is data or the keyboard is the application's
  decision; the API offers both, and the application chooses.
- **Do nothing.** The first consumer keeps `TerminalDriver::new()` and cannot
  support `cat f | mytui` at all, or documents that stdin must be the keyboard.
  That is the status quo this RFC exists to change.

## Impact

Additive, non-breaking. One public constructor is added to `TerminalDriver`
(`with_input(File)`), and the per-descriptor setup in `new()` is factored into a
helper that both constructors use; `new()`'s signature, documented errors, and
behavior are unchanged. Existing applications see no difference. No new
dependency: the caller opens `/dev/tty` (or any other terminal) and the crate
opens nothing new, so tuinix's runtime dependency set stays `libc` alone.

## Dependencies

The change is self-contained. It adds no dependency, so tuinix's runtime
dependency set stays `libc` alone and `rust-version` does not move. There is no
dependency on another backlog item.

## Testing

`with_input` takes the terminal as an argument, so it is testable over an
`openpty` pair without needing a real controlling terminal: open a pty, pass the
slave as `with_input(input)`, and assert that construction succeeds, that the
input descriptor is the slave, and that dropping the driver restores the slave's
termios. Passing a non-terminal (a pipe) must fail with the "not a terminal"
error.

`new()`'s own behavior is pinned by the existing tests and must not change; the
shared helper that backs both constructors is exercised by both paths.

- `open_nonblocking_input` already has unit tests over an `openpty` pair (a
  fresh, non-blocking description of a terminal, and rejection of a non-tty).
- The new tests above cover the `with_input` entry point directly, and
  specifically the pty-slave case that motivated taking a `File` rather than a
  `/dev/tty`-only constructor.
- An end-to-end check of the intended invocation (`printf a | binary`) needs a
  real controlling terminal and belongs in the consuming application's tests,
  not here.

## Unresolved questions

- **`with_input` takes `File` or `impl Into<OwnedFd>`?** `File` is the owned,
  unmissable choice and matches how the driver already stores its input
  (`File::from_raw_fd(..)`). `impl Into<OwnedFd>` would also accept a bare
  `OwnedFd`, which is more flexible but adds a generic to the signature for no
  caller the RFC knows of. Leaning `File`; settle at implementation time.
- **The shared helper's signature.** It should take the input descriptor (the
  existing `open_nonblocking_input(fd)` already does something similar). Whether
  `new()` is re-expressed through the same helper or kept parallel, sharing only
  `tcgetattr`/`ttyname_r`/raw-mode by descriptor, is an implementation detail --
  but `new()`'s input source (stdin) must not change either way.

## Future possibilities

- **A `from_*` convenience over `/dev/tty`.** If opening `/dev/tty` at every
  call site proves tedious, `from_controlling_terminal()` (or `from_tty()`) can
  be added later as a thin `with_input(File::open("/dev/tty")?)`. It is
  additive, so leaving it out now costs nothing and keeps the current API
  minimal.
- **Stdout is not a terminal.** The symmetric case -- `mytui > out`, or piping
  the TUI's own output -- is not addressed here. If it becomes wanted, an
  output-side constructor would be the place, and it would make the output
  descriptor a parameter the same way `with_input` makes the input one.
- **A generalized `with_io(input: File, output: ...)`.** If both directions
  become parameters, the constructors above are special cases of it. Not worth
  building speculatively, but it is where this naturally leads.
- **Reading stdin incrementally.** This RFC only needs the constructor; the
  first consumer reads stdin to the end before starting. A future pager that
  wants to stream stdin (like `less` does) would read it after entering raw
  mode, which is an application concern, not a driver one -- but it depends on
  the driver having taken its input from somewhere other than stdin, which is
  what this RFC provides.

## Outcome

Fill this in only when the proposal is settled.

## Outcome

Implemented in [#46](https://github.com/sile/tuinix/pull/46) (merged as `0d97690`).

The constructor landed as `with_input(File)`: `File` was kept over
`impl Into<OwnedFd>`, since the driver already stores its input as a `File` and
no caller needs the wider form.

The shared setup became a private `install(input, stdout, singleton)` that both
constructors call, so `new()` and `with_input()` differ only in where the input
`File` comes from. `new()` still derives its input from stdin -- it opens a
fresh, non-blocking description of the device stdin is connected to -- and its
contract and behavior are unchanged.

Two points settled differently from the text above:

- The stdout-is-a-terminal check moved into `install`, so both constructors run
  it once. `new()` keeps its own "STDIN is not a terminal" error, and the stdout
  error stays `Error::other`.
- `with_input` reports a non-terminal argument as `ErrorKind::InvalidInput`
  rather than `Error::other`, because the argument is the caller's and the kind
  names the fault.

Unlike `new()`, whose input comes from `open_nonblocking_input`, `with_input`
makes the caller's `File` non-blocking itself; this was not spelled out above
and is the one per-descriptor difference between the two paths.

The tests live in `src/terminal.rs`'s `mod tests`: `with_input_rejects_non_tty`
(runs everywhere), and `with_input_reads_from_the_given_terminal` /
`with_input_and_new_share_the_singleton` (skipped when stdout is not a
terminal).

The scope is unchanged from what is described above.
