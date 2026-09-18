---
Created: 2026-09-17
Status: draft
---

# RFC: Offer a helper for the standard poll loop

## Summary

Provide a small helper (or a documented module of recipes) that wraps the
`poll` loop every tuinix consumer ends up writing: drain ready input, retry on
`EINTR`, commit a lone `ESC` only after a timeout, and re-read interests between
rounds. The helper should be thin enough that a consumer with unusual needs can
ignore it and keep driving the loop itself.

## Motivation

Every non-trivial consumer of tuinix writes the same top-level loop. On a
Unix/`libc` target it looks like this:

```rust
loop {
    let mut fds = ...; // from every source's interests()
    let ready = libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout_ms);
    if ready < 0 {
        if errno == EINTR { continue; }
        // real error
    }
    if ready == 0 {
        // no fd is ready: this is the only safe time to commit a lone ESC,
        // because otherwise a following byte turns it into Alt+... instead
        if decoder.has_uncommitted_escape() {
            commit_escape(&mut decoder);
        }
        continue;
    }
    // translate ready fds into interests, feed the decoder, redraw
}
```

The loop has three subtle rules, and getting any of them wrong is a bug that is
not visible until the right keystroke or the right timing:

- **`EINTR` retry.** `poll` can return `-1`/`EINTR` on any signal; a loop that
treats that as fatal dies on `SIGWINCH`.
- **`ESC` commit only on timeout.** Committing a lone `ESC` as soon as the
  read returns splits `ESC A` (Alt+A) into `Escape`, then `A`. The bit that
  makes this correct — "only on `ready == 0`" — is easy to get backwards and
  is not stated where a caller will look for it.
- **Edge-triggered polling.** Readiness must be re-derived from current
  interests after every round; a loop that caches its fd set can miss a wakeup.
  tuinix documents this in prose, but the loop that has to obey it lives in the
  consumer.

The cost of not having a helper is that a consumer with an *unusual* loop
(extra fds, a timer, a second source) still has to write all of the above from
scratch, correctly, with no shared reference implementation to copy its
`EINTR`/`ESC`/interest handling from.

## Guide-level explanation

Before, a consumer writes the loop above itself, once per program, carefully.

After, a consumer that fits the common shape writes:

```rust
// shape only; names are prose
run_poll_loop(|| inputs.interests(), |ready| app.handle(ready), esc_timeout)
```

and a consumer with an unusual loop reads the same code, and the three rules
above, as a single documented function it can crib from instead of reinventing.

## Reference-level explanation

The shape worth extracting is deliberately narrow: "given a closure that
returns the current interests, and a closure that handles the ready subset, run
until the handler says stop". Anything wider (session management, rendering,
lifecycle) belongs to the consumer and should not be here.

Where it lives is an open question:

- **A function in tuinix.** tuinix already owns `Interests` and the input
  decoder that has the `ESC` hysteresis, so it is the natural owner. The cost
  is that tuinix would take a `libc` dependency (for `poll`) on Unix targets
  and would need a no-loop fallback (or a feature gate) everywhere else.
- **Documented recipes in `docs/` plus a worked example.** No new dependency,
  no new public API, but the code is still copied and can still drift from the
  rules.
- **The helper in a separate, small crate.** Keeps tuinix dependency-free; adds
  a fourth crate to the project.

The author leans toward the second: tuinix's value is being dependency-light
and Sans I/O, and a `libc` poll loop sits awkwardly inside it. A recipe plus a
well-commented example achieves the "copy this correctly" goal without making
tuinix own the runtime.

## Alternatives

### Do nothing; the consumer owns the loop

This is the status quo. It is defensible — the loop *is* the consumer's
runtime — but it means the three rules above are re-derived per project, and
the "only on timeout" rule in particular is the kind of thing that gets written
wrong once and then copied forward.

### Put the loop in a sibling crate that depends on tuinix

Viable, and the third option above. Deferred rather than rejected: if more than
one consumer wants the same loop, a crate is the honest place for it.

## Drawbacks

- A helper invites consumers to fit their loop to it rather than to their own
  needs, which is the opposite of what an unusual consumer should do.
- A `libc`-based loop in tuinix would tie a Sans I/O crate to a platform and a
  syscall, which is a real change in the crate's character.
- Documented recipes rot: nothing in CI keeps an example compiling against the
  current API unless it actually is a compiled example.

## Open questions

- Function in tuinix, recipes in `docs/`, or a separate crate (see above)?
- If a function: is `poll` the only mechanism, or should the helper also cover
  the self-pipe/eventfd wakeup a consumer with another source needs?
- Should the `ESC` timeout be a parameter, or does the decoder already own a
  recommended value that the helper should just use?
