# RFC: `TerminalDriver` writes the terminal's clipboard (OSC 52)

- Status: draft

## Summary

Add two methods to `TerminalDriver` that hand text to the terminal's clipboard
through OSC 52: `set_clipboard(&mut self, text: &str)` replaces what the
terminal holds, and `append_clipboard(&mut self, text: &str)` appends to it.
Both base64-encode the text and write one control sequence to the driver's
output, so an application can put text where the shell, another editor, or a
chat window can paste it -- including over SSH, where no clipboard helper is
available.

## Motivation

An application built on tuinix can cut text, but the text only reaches a buffer
inside the process. There is no way for it to leave: the operating system's
clipboard -- or, under tmux, the tmux buffer -- is a separate place, and the
only protocol an application has for reaching it is OSC 52 ("manipulate
selection data"). tuinix writes every other control sequence an application
needs -- alternate screen, raw mode, mouse reporting, the window title's
primitives through the terminal -- but not this one, so a consumer that wants it
must hand-roll the escape sequence, base64 and all, outside the driver where
the rest of the terminal control already lives.

This is not hypothetical: `kk` (a sibling project in this workspace's orbit)
has a proposal to export each cut to the terminal's clipboard, and the proposal
explicitly wants *tuinix* to own the protocol and the encoding, the way it owns
mouse reporting. See "Dependencies" below. Until tuinix offers the write, the
consumer has nothing to call and cannot land the feature.

The mechanism is old and portable. An application writes
`ESC ] 52 ; c ; <base64> ST` to the terminal; the terminal decodes the payload
and puts it on the system clipboard. It works over SSH, because it rides the
terminal connection rather than requiring local clipboard access, and tmux
forwards it to its own buffer when `set-clipboard` allows. tuinix's
`TerminalDriver` already writes raw sequences to its own output
(`enable_mouse_reporting` writes `\x1b[?1000h` and friends), so OSC 52 is the
same kind of write in the same place.

## Guide-level explanation

An application that wants a cut, a copy, or any other text to survive its own
process calls one of two methods:

```rust
use std::io::Write;

fn main() -> std::io::Result<()> {
    let mut driver = tuinix::TerminalDriver::new()?;

    // Usually a bug report or a logs snippet, but any text will do.
    // Replaces whatever the terminal holds.
    driver.set_clipboard("hello from tuinix")?;

    // Appends to what is already there. A caller that collects a run of
    // pieces appends the later ones.
    driver.append_clipboard("\nsecond line")?;

    driver.flush()?;
    Ok(())
}
```

Both methods buffer into the driver's existing output, so nothing reaches the
terminal until the caller flushes -- the same contract as every other write
through the driver, including the frame bytes the application renders.

There is no new key, no mode, and no new state on the driver beyond what it
already carries. The methods are best-effort in a way that is intrinsic to
OSC 52 and is not a tuinix shortcoming: the terminal either accepts the
sequence or discards it, and *it cannot be asked which*. There is no reply.
A terminal may accept the bytes and still have nowhere to put them (a headless
server, the feature configured off), and tmux may decline to forward it. So the
methods do not report whether the clipboard actually changed. `Ok(())` means
the sequence was written, not that it was honored.

## Reference-level explanation

```rust
impl TerminalDriver {
    /// Replaces the terminal's clipboard with `text`, using OSC 52.
    ///
    /// Encodes `text` as base64 and writes
    /// `ESC ] 52 ; c ; <base64> ST` to the terminal's output. The sequence is
    /// buffered like any other write, so the caller flushes (or writes a frame)
    /// for it to take effect.
    ///
    /// This is best-effort: OSC 52 has no reply, so a terminal that ignores the
    /// sequence, or has no clipboard behind it, is indistinguishable from one
    /// that accepts it. `Ok(())` means the sequence was written, not delivered.
    ///
    /// # Errors
    ///
    /// Returns an error only if writing to the terminal's output fails.
    pub fn set_clipboard(&mut self, text: &str) -> io::Result<()>;

    /// Appends `text` to the terminal's clipboard, using OSC 52.
    ///
    /// Like [`Self::set_clipboard`], but the selection argument is left empty
    /// (the OSC 52 append form) so the terminal appends rather than replaces.
    /// The same best-effort and buffering notes apply.
    ///
    /// # Errors
    ///
    /// Returns an error only if writing to the terminal's output fails.
    pub fn append_clipboard(&mut self, text: &str) -> io::Result<()>;
}
```

### The sequence

A set writes:

```text
ESC ] 52 ; c ; <base64(text)> ST
```

- `ESC ]` (`0x1b 0x5d`) opens an OSC. tuinix already recognizes this introducer
  on the *input* side (see `docs/input-decoding.md`); here it is on the output
  side.
- `52` selects "manipulate selection data".
- `c` names the **system clipboard** selection. OSC 52 can also address the
  primary selection and others; this API exposes only the system clipboard,
  because that is what "paste into another window" means everywhere.
- The payload is the base64 of the text's UTF-8 bytes.
- `ST` (`ESC \`, `0x1b 0x5c`) terminates the sequence. `BEL` (`0x07`) is also
  accepted by many terminals as an OSC terminator, but `ST` is the correct one
  and the one tuinix writes. One choice, stated here, rather than a behavior
  that varies by terminal.

An append writes the same thing with the selection argument omitted:

```text
ESC ] 52 ; ; <base64(text)> ST
```

The empty field between the two semicolons is what tells the terminal to append
to the current clipboard instead of replacing it. This is the standard's own
distinction, and it mirrors the set/append pair the consumer already has for its
in-process clipboard.

### Where the code goes

The two methods live in `src/terminal.rs`, beside `enable_mouse_reporting` /
`disable_mouse_reporting`, and write to `self.output` before flushing, exactly
as those do:

```rust
pub fn set_clipboard(&mut self, text: &str) -> io::Result<()> {
    write!(self.output, "\x1b]52;c;{}", base64(text))?;
    write!(self.output, "\x1b\\")?;
    self.output.flush()?;
    Ok(())
}
```

(The exact split between writing the body and the terminator is an
implementation detail; the bytes are what the sections above say.)

### Base64 belongs in tuinix, in this module

tuinix has no base64 dependency today, and adding one for this is a cost the
crate's "minimum dependencies" charter does not obviously justify. The encoder
this needs is small -- a 64-character alphabet, three input bytes to four output
characters, `=` padding -- and there is no decoder to write, because tuinix only
produces the payload. The RFC's preference is to write the ~30-line encoder as a
private helper in `src/terminal.rs` (or a small private module) rather than pull
in a crate. That keeps `Cargo.toml`'s single runtime dependency (`libc`) intact
and keeps the one consumer of the encoding private to the one place that needs
it. If the encoder ever grows a second caller, extracting it (or taking the
dependency) is a separate decision.

### Size cap

A base64 payload is written as one sequence, and terminals vary in how large a
sequence they accept -- some cap the sequence, some cap the clipboard, some drop
a sequence past a length. A cut could be megabytes, so an unbounded write is a
footgun. Both methods refuse to export text past a hard-coded cap of **10 MiB**
(`10 * 1024 * 1024` bytes of input text; base64 makes the sequence about a
third larger). Past the cap the text is not written and the call returns
`Ok(())` -- the same silent skip as every other unobservable failure here --
because the caller still holds the text and only the outside copy is skipped.
The cap is checked before encoding, so no work is done for text that will not be
sent. Making it configurable is deliberately out of scope.

The cap lives with the encoding, in these methods, so every caller gets the same
bound and no caller has to remember it.

### Failure is silent, and that is correct

Mouse reporting sets an error message when it is unavailable; OSC 52 cannot be
made to say anything as useful. By the time the sequence is written there is no
failure to observe: the terminal accepts or discards, and does not say which.
There is no OSC 52 reply to wait for, and querying terminal support through DA1
and similar is not reliable for this. So these methods do not report support,
do not set state, and do not surface a message. The `io::Result` return is only
for the same reason every driver write has one -- the output descriptor can
fail -- not as a signal about the clipboard. This is stronger than mouse
reporting's report-and-swallow, and it is deliberate: there is nothing to
report, and inventing a message would be guessing.

## Drawbacks

- New public API on the crate's central type, for a feature whose success is
  never observable to the caller. "It worked" and "the terminal ignored it"
  both return `Ok(())`.
- A base64 encoder becomes new code in the crate (or a new dependency, which the
  RFC argues against). Either is a cost for a sequence no test in tuinix itself
  can fully validate as delivered.
- A size cap and an alphabet are two more constants to keep true.
- The append form (empty selection argument) is a small amount of protocol
  subtlety that a caller who only sets will never exercise, but that has to be
  gotten right for the callers that do.

## Rationale and alternatives

- **The consumer builds the sequence.** That would leave tuinix nearly
  untouched and put base64 plus OSC 52 in the application. Rejected: tuinix
  owns every other terminal control sequence, including mouse reporting and
  the resize signal, and protocol in the consumer is a second place that knows
  escape codes -- one that other tuinix users cannot reuse. Keeping protocol in
  the driver and intent in the application is the crate's existing division.
- **A single `set_clipboard` with no append.** Simpler by one method, but a
  consumer that collects a run of pieces (the shape `kk` cuts in) would have to
  read back the clipboard to append to it, and OSC 52 has no read. Without the
  append form, the consumer would end up sending only the last piece. Two
  methods, mirroring the replace/append split the consumer already has, is the
  honest shape.
- **Take a `selection` argument (`c`, `p`, ...) as the API does today.** The
  third field of OSC 52 names a selection. Exposing it is more surface than the
  first version needs; the system clipboard is what "paste elsewhere" means, and
  an application that wants another selection can be served in a follow-up.
- **Take `impl AsRef<[u8]>` instead of `&str`.** The payload is base64 of bytes
  and the protocol does not care that they are valid UTF-8. But clipboard text
  from a text editor is text, `&str` says so, and a byte-slice signature would
  invite non-text payloads (an image, a binary) that the size cap and the
  framing were not designed around. `&str` is the narrower, correct contract.
- **Return whether the sequence was written, or a `Result` with a
  "not-supported" error.** There is nothing to return it about: the write
  either reaches the output descriptor or fails at the descriptor (already an
  `io::Error`), and whether the *terminal* honors it cannot be known. A support
  error would be fabricated.
- **Let the application decide the cap, or drop it entirely.** Configurability
  is more surface than the first version needs; dropping it leaves a
  multi-megabyte cut writing a multi-megabyte sequence into terminals that will
  drop it. A fixed cap in one place is the smallest correct choice.
- **Do nothing.** The status quo: a tuinix consumer that wants OSC 52 writes it
  itself, duplicating protocol that belongs in the driver. This is exactly the
  case the crate is meant to cover.

## Impact

Additive, non-breaking. Two public methods and a private helper (or a small
private module) are added to `TerminalDriver`; no existing signature changes and
no existing behavior moves. The crate's runtime dependency set stays `libc`
alone under the RFC's preferred design. An application that never calls the new
methods is unaffected, and a terminal that ignores OSC 52 sees the new methods
as a no-op on its side.

## Dependencies

This API is the tuinix half of a two-crate feature. The other half lives in a
consumer (`kk`) that wants to export each cut to the terminal's clipboard; its
proposal depends on `set_clipboard` / `append_clipboard` existing here, because
`kk` can only write to the terminal through `TerminalDriver`. The consumer's own
RFC says so and treats tuinix as the required prerequisite.

There is a third piece, `termnix` (the test terminal). It already delivers OSC 52
to its caller: `TerminalState::take_osc_request()` returns an
`OscRequest::SetClipboard { text, selection, append }` whose `text` is the
decoded payload. A test can therefore assert the *decoded* clipboard text -- that
the base64 decoded back to what was written, and that a set was told apart from
an append -- by driving tuinix through a real PTY-backed `termnix` session and
draining that channel.

That test is where this RFC's implementation needs `termnix`, and it needs it as
a **dev-dependency of tuinix**: tuinix does not depend on `termnix` today, in any
form. Adding `termnix` as a dev-dependency is therefore part of implementing this
RFC -- it does not affect downstream consumers, but it is what lets the change
land with a test that checks the encoding rather than only the raw bytes.

One consequence has to be settled when the dependency is added: the released
`termnix` requires a newer MSRV than tuinix's current `rust-version`, so taking
it as a dev-dependency raises the toolchain tuinix's own `cargo build
--all-targets` needs (and the version CI's MSRV job pins). That is a decision for
the implementing change, not for the API, but it is part of the same step.

Filing items against the consumer is a separate step and is not a prerequisite
for this tuinix change.

## Unresolved questions

- **Encoder vs. dependency.** The RFC prefers a private ~30-line encoder over a
  new crate. This is a judgment, not a settled fact; if a maintainer prefers a
  small `base64` dependency for correctness confidence, the API does not change.
- **Split of the sequence.** Whether the terminator is written in the same
  `write!` as the body is an implementation detail; the RFC fixes the bytes, not
  the formatting.
- **Terminator choice.** `ST` (`ESC \`) is chosen over `BEL`. Both are legal for
  OSC; `ST` is the correct one. If a real terminal is found that needs `BEL`,
  that is a data point to revisit, not a silent fallback.

## Future possibilities

- A `selection` argument for the primary/other selections, if the fixed system
  clipboard proves too narrow.
- A configurable size cap, if a fixed 10 MiB is wrong for some consumer.
- Reading the clipboard, if a terminal ever grows an OSC 52 reply; today there
  is none, which is why the append method exists instead of a read-modify-write.
- If more than one consumer wants the encoding, extracting the base64 helper
  (or taking the dependency) becomes worthwhile, and this RFC's private-helper
  choice would be the thing to revisit.
