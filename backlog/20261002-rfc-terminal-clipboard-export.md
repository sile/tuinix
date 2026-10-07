# RFC: `TerminalDriver` writes the terminal's clipboard (OSC 52)

- Status: draft

## Summary

Add one method to `TerminalDriver`, `set_clipboard(&mut self, text: &str)`, that
puts `text` on the terminal's clipboard through OSC 52. It base64-encodes the
text and writes one control sequence to the driver's output, so an application
can put text where the shell, another editor, or a chat window can paste it --
including over SSH, where no clipboard helper is available.

## Motivation

An application built on tuinix can cut text, but the text only reaches a buffer
inside the process. There is no way for it to leave: the operating system's
clipboard -- or, under tmux, the tmux buffer -- is a separate place, and the
only protocol an application has for reaching it is OSC 52 ("manipulate
selection data"). tuinix writes every other control sequence an application
needs -- alternate screen, raw mode, mouse reporting -- but not this one, so a
consumer that wants it must hand-roll the escape sequence, base64 and all,
outside the driver where the rest of the terminal control already lives.

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
process calls one method:

```rust
use std::io::Write;

fn main() -> std::io::Result<()> {
    let mut driver = tuinix::TerminalDriver::new()?;

    // Usually a bug report or a log snippet, but any text will do.
    // Replaces whatever the terminal holds.
    driver.set_clipboard("hello from tuinix")?;

    driver.flush()?;
    Ok(())
}
```

The method buffers into the driver's existing output, so nothing reaches the
terminal until the caller flushes -- the same contract as every other write
through the driver, including the frame bytes the application renders.

There is no new key, no mode, and no new state on the driver beyond what it
already carries. The method is best-effort in a way that is intrinsic to OSC 52
and is not a tuinix shortcoming: the terminal either accepts the sequence or
discards it, and *it cannot be asked which*. There is no reply. A terminal may
accept the bytes and still have nowhere to put them (a headless server, the
feature configured off), and tmux may decline to forward it. So the method does
not report whether the clipboard actually changed. `Ok(())` means the sequence
was written, not that it was honored.

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

### Where the code goes

The method lives in `src/terminal.rs`, beside `enable_mouse_reporting` /
`disable_mouse_reporting`, and writes to `self.output` before flushing, exactly
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

### No size cap

The method places no limit on the text's length. OSC 52 payloads are UTF-8 text
and terminal clipboards are not, in practice, larger than a cut of a screenful
or a file; a caller that wants a bound on what it exports can check the length
itself before calling, which is one line where the policy belongs. Putting a cap
in the driver would bake an arbitrary number into the API and force every caller
through it.

### Failure is silent

Mouse reporting writes its sequences the same way and reports nothing, and
OSC 52 has even less to go on: by the time the sequence is written the terminal
accepts or discards it, and does not say which. There is no OSC 52 reply to wait
for, and probing terminal support through DA1 and similar is not reliable for
this. So the method does not report support, does not set state, and does not
surface a message. The `io::Result` return is only for the same reason every
driver write has one -- the output descriptor can fail -- not as a signal about
the clipboard.

## Drawbacks

- New public API on the crate's central type, for a feature whose success is
  never observable to the caller. "It worked" and "the terminal ignored it"
  both return `Ok(())`.
- A base64 encoder becomes new code in the crate (or a new dependency, which the
  RFC argues against). Either is a cost for a sequence no test can fully
  validate as delivered.

## Rationale and alternatives

- **The consumer builds the sequence.** That would leave tuinix nearly
  untouched and put base64 plus OSC 52 in the application. Rejected: tuinix
  owns every other terminal control sequence, including mouse reporting and
  the resize signal, and protocol in the consumer is a second place that knows
  escape codes -- one that other tuinix users cannot reuse. Keeping protocol in
  the driver and intent in the application is the crate's existing division.
- **An `append` variant (empty selection field).** OSC 52 can append to the
  current clipboard by leaving the selection argument empty. Left out: no
  consumer needs it today, and adding it later is a one-line, non-breaking
  change to the same method family. A first version that does the whole of what
  is asked for, and nothing more, is the better starting point.
- **Take a `selection` argument (`c`, `p`, ...).** The third field of OSC 52
  names a selection. Exposing it is more surface than the first version needs;
  the system clipboard is what "paste elsewhere" means, and an application that
  wants another selection can be served in a follow-up.
- **Take `impl AsRef<[u8]>` instead of `&str`.** The payload is base64 of bytes
  and the protocol does not care that they are valid UTF-8. But clipboard text
  from a text editor is text, `&str` says so, and a byte-slice signature would
  invite non-text payloads (an image, a binary) that the framing was not
  designed around. `&str` is the narrower, correct contract.
- **Cap the input length.** See "No size cap" above: a policy bound belongs at
  the caller that owns the policy.
- **Do nothing.** The status quo: a tuinix consumer that wants OSC 52 writes it
  itself, duplicating protocol that belongs in the driver. This is exactly the
  case the crate is meant to cover.

## Impact

Additive, non-breaking. One public method and a private helper (or a small
private module) are added to `TerminalDriver`; no existing signature changes and
no existing behavior moves. The crate's runtime dependency set stays `libc`
alone under the RFC's preferred design. An application that never calls the new
method is unaffected, and a terminal that ignores OSC 52 sees the new method as
a no-op on its side.

## Dependencies

This API is the tuinix half of a two-crate feature. The other half lives in a
consumer (`kk`) that wants to export each cut to the terminal's clipboard; its
proposal depends on `set_clipboard` existing here, because `kk` can only write
to the terminal through `TerminalDriver`. The consumer's own RFC says so and
treats tuinix as the required prerequisite.

The change is otherwise self-contained. It adds no dependency: the encoder is a
private helper in this crate (see "Base64 belongs in tuinix" above), so
tuinix's runtime dependency set stays `libc` alone and its `rust-version` does
not move. Testing it needs nothing outside the crate either -- the sequence is a
fixed byte string and the encoding is checked against fixed vectors (see
"Testing" below).

Filing items against the consumer is a separate step and is not a prerequisite
for this tuinix change.

## Testing

The change is testable inside the crate, without a PTY and without a new
dependency:

- The written bytes are a fixed string. A test drives the method against a
  `Vec<u8>`-backed writer (or a driver built over one) and asserts the exact
  sequence: `ESC ] 52 ; c ; <b64> ST`. This pins the framing -- introducer,
  `52`, the selection field, the terminator -- which is the part a reader of the
  API most needs to stay still.
- The encoder is checked against fixed vectors with known-correct base64, chosen
  to cover the boundaries: an empty input, inputs of length `3n-2`, `3n-1` and
  `3n` (so all three padding cases), and an input whose bytes need the full
  alphabet. Expected outputs are written out literally, not computed by the same
  helper, so a single shared bug cannot pass both sides.

What these tests cannot show is whether a real terminal honors the sequence.
There is no reply to observe, so that question is out of reach for any test;
the API's contract (see "Failure is silent") is written to match that, rather
than to be validated by a test that cannot exist.

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

- An `append_clipboard` method (empty selection field), if a consumer needs to
  add to the clipboard instead of replacing it.
- A `selection` argument for the primary/other selections, if the fixed system
  clipboard proves too narrow.
- Reading the clipboard, if a terminal ever grows an OSC 52 reply; today there
  is none.
- If more than one consumer wants the encoding, extracting the base64 helper
  (or taking the dependency) becomes worthwhile, and this RFC's private-helper
  choice would be the thing to revisit.
