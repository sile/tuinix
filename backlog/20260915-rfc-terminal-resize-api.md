# RFC: Split terminal size into a pure getter and an explicit resize handler

- Status: accepted

## Summary

Rework the size-related methods of `TerminalDriver` so that reading the size
and reacting to a resize are two separate operations. `size()` becomes a
`&self` getter that returns the last observed size and does no IO, and
`handle_resize_signal()` becomes the only operation that drains resize
notifications and re-queries the terminal.

## Motivation

The current `TerminalDriver` mixes three roles into one type:

1. owning terminal modes (raw mode, alternate screen, mouse reporting) — output
   IO,
2. caching the size (`cached_size`) — pure state, and
3. subscribing to resize notifications (`signal_fd`, draining the descriptor) —
   an event source.

The size methods themselves blur two operations. `size(&mut self) ->
io::Result<Size>` is named like a getter but is not one: it drains the signal
descriptor, conditionally re-queries the terminal, and returns the cache. A
caller cannot ask for the size without also consuming notifications, and
cannot consume notifications without also asking for the size. The two
concerns are locked together.

This shows up when the caller already owns an event loop. Such a caller
registers `signal_fd()` for readiness, and when it becomes readable the
caller calls `size()` to observe the new value. The call then re-drains the
descriptor that the caller already knows is readable, which makes the
readiness check feel redundant and the division of labor unclear: who is
responsible for noticing a resize, the caller or the driver?

There is also an asymmetry with the rest of tuinix. `Frame::size` is a
`&self` getter that returns a plain `Size`. `TerminalDriver::size` is the odd
one out: `&mut self` and fallible, for reasons that are about notification
draining rather than about reading a size.

## Guide-level explanation

The two operations become explicit and named for what they do:

- `size(&self) -> Size` — read the last observed size. No IO, infallible,
  idempotent. Use it whenever you need the current size.
- `handle_resize_signal(&mut self) -> io::Result<()>` — handle a resize
  notification.
  Register `signal_fd()` with your event loop; when it becomes readable, call
  this. It drains the descriptor and, if a notification was present, re-queries
  the terminal and updates the cache. Afterwards `size()` returns the new
  value.

A caller with an event loop:

```rust
// once, at startup
loop.add(driver.signal_fd(), Interest::READABLE)?;

// whenever the signal descriptor becomes readable
if event.is_readable() {
    driver.handle_resize_signal()?;
    frame.resize(driver.size());
}
```

A caller that never watches the descriptor still gets a usable size:

```rust
let size = driver.size();
```

Note that `handle_resize_signal` returns no size and does not report whether
the size
actually changed. If a caller needs "did it change", it compares `size()`
before and after — a notification can arrive without the size differing.

## Reference-level explanation

```rust
impl TerminalDriver {
    /// Returns the last observed terminal size.
    ///
    /// Performs no IO and does not detect resizes. The value is refreshed only
    /// by [`Self::handle_resize_signal`], so a caller that never watches the
    /// signal
    /// descriptor observes the size the terminal had at construction time.
    pub fn size(&self) -> Size;

    /// Consumes a pending resize notification, if any.
    ///
    /// Drains the signal descriptor. If at least one notification was
    /// pending, re-queries the terminal and updates the cached size, so that
    /// a subsequent [`Self::size`] returns the new value. If nothing was
    /// pending, does nothing and returns `Ok(())`.
    ///
    /// Intended to be called when the descriptor from [`Self::signal_fd`]
    /// becomes readable. Calling it when nothing is pending is harmless.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal size cannot be re-queried after a
    /// resize notification.
    pub fn handle_resize_signal(&mut self) -> io::Result<()>;

    /// Returns a descriptor that becomes readable when the terminal resizes.
    pub fn resize_signal_fd(&self) -> RawFd;
}
```

Key points:

- The old `size(&mut self) -> io::Result<Size>` is removed, not deprecated
  under a new name, so there are no two methods called `size` with different
  meanings.
- `size()` does no IO, so it cannot fail; it returns a plain `Size`, matching
  `Frame::size`. `TerminalDriver` always holds a size because construction
  queries the terminal once and fails if that fails.
- `handle_resize_signal` returns `()` rather than `bool`. A `bool` would report
  whether
  a notification was consumed — information the caller already has, because it
  watched the descriptor to get here. It would not answer the question callers
  actually ask, "did the size change", which requires comparing `size()`
  anyway. Returning nothing keeps the two notions distinct and lets the
  common path use `?`.
- A missing notification is not an error, so `handle_resize_signal` does not
  return
  `WouldBlock`. Nothing is pending, nothing happens, `Ok(())`. This keeps the
  call idempotent and avoids asking the caller to special-case an error kind
  for a normal outcome.
- `resize_signal_fd` is the existing `signal_fd` under a name that says what
  the descriptor carries, and `handle_resize_signal` is named for the signal
  that arrives on it. The fd and the handler share the token `resize_signal`,
  so the pair reads consistently: watch `resize_signal_fd`, then handle the
  resize signal.

## Drawbacks

- **This is a breaking change.** `size()` changes signature and there are now
  two operations where there was one. Removal of
  `size(&mut self) -> io::Result<Size>` breaks every caller.
- **Silent staleness.** `size()` still returns `Size`, so a caller that never
  calls `take_resize` keeps compiling and gets a value that never changes.
  The old `size()` would have refreshed on each call. The migration has to be
  clear about this.
- **Two methods where one sufficed.** For a caller with no event loop and no
  interest in resizes, the old single `size()` was simpler than remembering to
  call `take_resize`.

## Rationale and alternatives

- **Why prefer this over keeping `bool` return on a resize handler?** Because
  "was a notification consumed" is already known to the caller that watched
  the descriptor, and it is a different question from "did the size change".
  `bool` answers the former, which callers do not need, and cannot answer the
  latter. `()` plus a doc note is honest about what the method does.
- **Why not return `Size` or `Option<Size>` from `handle_resize_signal`?** The
  size is
  available from `size()` afterwards, so returning it duplicates state and
  invites callers to ignore `size()`. `Option` additionally suggests the size
  may be absent, but `TerminalDriver` always holds one.
- **Why not `WouldBlock` when nothing is pending?** It is a normal outcome,
  not a failure. `WouldBlock` would force every caller to branch on an error
  kind for the ordinary case and to reason about re-registration ("should I
  keep watching the descriptor?"), when the intended policy is to leave read
  interest registered and call `handle_resize_signal` whenever it fires.
  Normalizing
  the empty case to `Ok(())` keeps that policy simple.
- **Alternative name: `size_if_notified`.** An earlier draft kept a single
  `&mut` method that returned the size only when notified. Rejected: it still
  couples reading the size to draining notifications, and a conditional getter
  reads less clearly than an explicit operation plus a plain getter.
- **Alternative name: `poll_size`.** Rejected: `poll` in tuinix already means
  fd readiness, and this method does ioctl work, not polling; the name would
  collide with the vocabulary used to describe the event-loop integration.
- **Alternative name: `take_resize`.** An earlier draft named the handler
  after the value it consumes. Rejected: the method handles a signal that
  arrives on a descriptor, not a stored value, and `take_resize` shares no
  token with `resize_signal_fd`. `handle_resize_signal` names the event and
  matches the fd accessor.
- **Alternative name: `take_resize_notification`.** Clearer than `take_resize`
  but longer, and still frames the method as consuming a value rather than
  handling a signal.
- **Alternative: add `cached_size(&self) -> Size` and keep `size(&mut
  self)`.** Rejected as a non-breaking but worse endpoint: it leaves two size
  accessors with an unclear split and keeps the asymmetry with `Frame::size`.
- **Why `handle_resize_signal` rather than `take_resize`?** The handler
  consumes what the descriptor delivers — a resize signal — so naming it after
  that signal lets it share the token `resize_signal` with `resize_signal_fd`.
  Reading the two together ("watch the resize signal fd, then handle the
  resize signal") needs no gloss. `take_resize` mixed the descriptor's
  vocabulary with a value-consuming verb and matched the fd only on the bare
  word `resize`.
- **Alternative: keep `signal_fd` and add `handle_resize_signal`.** Rejected:
  it unifies the verbs but leaves the fd accessor under the generic
  `signal_fd`, so the pair still does not share a token. Renaming the accessor
  to `resize_signal_fd` is what makes the pairing work.
- **Do nothing.** The current API is usable but forces notification draining
  into every size read and leaves `TerminalDriver::size` inconsistent with
  `Frame::size`. There is no way to read the size without consuming
  notifications, which is the actual complaint.

## Unresolved questions

- None. The method set (`size`, `handle_resize_signal`, `resize_signal_fd`)
  and the `()` return for `handle_resize_signal` are settled.

## Future possibilities

- If tuinix ever grows an event-loop helper, this pair is the primitive such a
  loop would call: watch `resize_signal_fd`, then call
  `handle_resize_signal`.
- An explicit "re-query now" method (for a terminal that changed size without
  a SIGWINCH) was considered and left out; it is not needed by any known use
  case and can be added later without disturbing this design.
