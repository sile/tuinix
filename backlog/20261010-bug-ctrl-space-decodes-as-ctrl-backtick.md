# Bug: Ctrl+Space (`0x00`) decodes as Ctrl+backtick

- Status: open

## Summary

Pressing Ctrl+Space sends the single byte `0x00` (NUL), but the decoder never
reports it as Ctrl+Space. `0x00 < 0x20` falls into the generic control-character
arm of `parse_ascii_char`, which computes `(0x00 + 0x60) as char` and so produces
`KeyInput { ctrl: true, alt: false, code: Char('`'), }` -- Ctrl+backtick. The
documented table lists `0x01..=0x1f` as the bytes that decode to a Ctrl chord
and is silent on `0x00`, so the value is decoded into a key that was never
pressed rather than being reported.

## Reproduction

Feed the single byte `0x00` to the decoder:

```text
TerminalDriver (or InputDecoder) fed 0x00
observed: Input::Key(KeyInput { ctrl: true, alt: false, code: KeyCode::Char('`') })
```

The Alt form is affected the same way. Feeding `0x1b 0x00` takes the
`bytes[1] < 0x20` branch of `parse_alt_char` and again computes
`(0x00 + 0x60) as char`:

```text
observed: Input::Key(KeyInput { ctrl: true, alt: true, code: KeyCode::Char('`') })
```

The bug is independent of how the bytes are split: a bare `0x00` is decoded on
the first byte, so there is no incomplete-prefix interaction.

## Observed behavior

`parse_ascii_char` in `src/input.rs` handles bytes below `0x20` with:

```rust
if byte < 0x20 {
    let (ctrl, code) = match byte {
        0x0D => (false, KeyCode::Enter), // Enter
        0x09 => (false, KeyCode::Tab),   // Tab
        c => (true, KeyCode::Char((c + 0x60) as char)),
    };
    return (Some(create_key_input(ctrl, false, code)), 1);
}
```

The catch-all `c => (true, KeyCode::Char((c + 0x60) as char))` is written for the
letters: it turns `0x01` into `Char('a')`, `0x1a` into `Char('z')`. For `0x00`
the same arithmetic yields `0x60`, which is `` ` ``, so the decoder reports a key
the user did not press. `parse_alt_char` has the same arm for `bytes[1] < 0x20`
and produces Alt+Ctrl+backtick for `0x1b 0x00`.

The invariant that breaks is on the reader's side: every key the decoder reports
is supposed to be one the terminal could have sent for a key that exists. A
terminal that sends `0x00` means Ctrl+Space; no terminal sends `0x00` for
Ctrl+backtick.

## Expected behavior

`0x00` is Ctrl+Space (equivalently Ctrl+`@`), the same key a terminal sends for
both. The decoder should report
`Input::Key(KeyInput { ctrl: true, alt: false, code: KeyCode::Char(' ') })` for a
bare `0x00`, and the Alt form should report the same code with `alt: true` for
`0x1b 0x00`.

## Impact

A correctness problem, reproducible from the public API: any consumer that
binds Ctrl+Space -- a common prefix or mode key -- can never see it. Instead it
receives a Ctrl+backtick that the user never pressed, so the binding either
never fires or fires on the wrong key. Feeding `0x00` to the decoder is enough
to see it, so the bug is reachable through the public API alone.

No resource effect; the input is decoded and consumed as one byte either way.

## Notes

Both arms share one cause and should be fixed together. When adding `0x00`,
confirm what the Alt path should do with a control byte whose modifier the
keyboard reports differently: `0x1b 0x00` is the Alt+Ctrl+Space the terminal
can actually send, so it should keep `ctrl: true` as well as `alt: true`, the
way `parse_alt_char` already does for the letters.
