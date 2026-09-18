# RFC: Offer a helper for the standard poll loop

- Status: postponed

## Summary

Provide a small helper (or a documented module of recipes) that wraps the
`poll` loop every tuinix consumer ends up writing: drain ready input, retry on
`EINTR`, commit a lone `ESC` only after a timeout, and re-read interests between
rounds. The helper should be thin enough that a consumer with unusual needs can
ignore it and keep driving the loop itself.

Postponed. The rules are real, but the crate documentation now shows the whole
loop in one place, and there is not yet a consumer that needed the same loop
twice. See `## Outcome`.

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

This was written when the crate documentation only showed the first rule. It
has since grown the full loop: the example in the crate root retries on
`EINTR`, switches the `poll` timeout on `has_uncommitted_escape()`, commits the
lone `ESC` when the timeout fires, and bails out on `POLLHUP`/`POLLERR`/
`POLLNVAL`. A consumer that gets the rules wrong by copying a `poll(...)` line
from the README is pointed at that example and at the demo, which is a
compiled version of the same loop. What is still missing is not a reference
implementation; it is evidence that a consumer needs the same loop a second
time.

## Guide-level explanation

If a helper existed, a consumer that fits the common shape would write:

```rust
// shape only; names are prose
run_poll_loop(|| inputs.interests(), |ready| app.handle(ready), esc_timeout)
```

and a consumer with an unusual loop would read the same code, and the three
rules above, as a single documented function it could crib from instead of
reinventing. What a consumer does today is copy the loop from the crate-root
example, which is the same trade with one fewer moving part.

## Reference-level explanation

The shape worth extracting is deliberately narrow: "given a closure that
returns the current interests, and a closure that handles the ready subset, run
until the handler says stop". Anything wider (session management, rendering,
lifecycle) belongs to the consumer and should not be here.

Where such a thing would live was the question this RFC weighed, and the three
answers are recorded here because they are what the next attempt would compare:

- **A function in tuinix.** tuinix already owns `Interests` and the input
  decoder that has the `ESC` hysteresis, so it is the natural owner. The cost
  is that tuinix would take a `libc` dependency (for `poll`) on Unix targets
  and would need a no-loop fallback (or a feature gate) everywhere else.
- **Documented recipes in `docs/` plus a worked example.** No new dependency,
  no new public API, but the code is still copied and can still drift from the
  rules. This has partly happened already, in the form of the crate-root
  example, without adding a `docs/` page: the two `docs/` pages that exist are
  behavioral references, not recipes, and a recipe is better served by code
  that actually compiles.
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
wrong once and then copied forward. This is the answer for now: the cost of
sharing a reference implementation was paid by documenting the loop instead of
by handing out a function, and until a consumer copies it more than once there
is nothing for a helper to save.

### Put the loop in a sibling crate that depends on tuinix

Viable, and the third option above. Deferred rather than rejected: if more than
one consumer wants the same loop, a crate is the honest place for it.

## Drawbacks

- A helper invites consumers to fit their loop to it rather than to their own
  needs, which is the opposite of what an unusual consumer should do.
- A `libc`-based loop in tuinix would tie a Sans I/O crate to a platform and a
  syscall, which is a real change in the crate's character.
- Documented recipes rot: nothing in CI keeps an example compiling against the
  current API unless it actually is a compiled example. This is the argument
  for putting the loop in the crate-root example rather than in a `docs/`
  page, and it is why the recipe option was realized that way.

## Outcome

Postponed. No helper, no separate crate, and no new `docs/` page. The three
rules are real and the Motivation was accurate about them, but the part of the
problem that could be closed now was not the API — it was that the crate
documentation showed only the first rule while asking the reader to obey all
three. That was fixed by writing the full loop into the crate-root example:
it retries on `EINTR`, picks the `poll` timeout from
`has_uncommitted_escape()`, calls `commit_escape()` when the timeout fires, and
returns early on `POLLHUP`/`POLLERR`/`POLLNVAL`. The README keeps its short
loop and now points at that example and at `examples/demo.rs`, which is the
same loop in compiled form. Because the example is a doctest, CI keeps it
compiling, which was the main drawback of the recipe option.

What is left is a judgment about demand, not about design. Extract a helper
when a consumer needs the same loop a second time — the same program driving
two loops, a second program copying `examples/demo.rs`, or a report of a loop
that got the `ESC` timeout wrong after copying it from somewhere. Until then,
there is nothing for the helper to save and no shape to extract it from: the
only loop anyone has written is one program's, and a helper built from one
caller tends to take that caller's structure as the general case.

The `libc` and Sans I/O argument also stands on its own. A `poll` loop in
tuinix would take a platform and a syscall dependency to save code that a
consumer can already copy from a compiled example. If the demand does appear,
the separate crate is the more honest home, since it can depend on whatever
the loop needs without changing what tuinix is.

The scope is unchanged from what is described above.

## Unresolved questions

- None; settled as postponed when the item was decided. The questions that were
  here (function vs. recipe vs. separate crate, `poll` vs. other wait
  mechanisms, parameterized vs. fixed `ESC` timeout) are all answered by the
  Outcome's resume condition: they belong to whoever extracts the helper, once
  there is a second caller to extract it from.
