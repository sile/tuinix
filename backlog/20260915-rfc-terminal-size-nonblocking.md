# RFC: Add a size query that consumes a pending resize notification

- Status: draft

## Summary

Add a method that returns the terminal size only when a resize notification is
pending, and reports `None` otherwise, so a caller that already polls the
signal descriptor can decide *when* to re-query the terminal instead of having
`size()` do it on its own.

## Motivation

The current `TerminalDriver::size` takes `&mut self` because it drains pending
resize notifications and re-queries the terminal when at least one was
received. The doc is explicit about this, so the behavior is disclosed and not
hidden policy.

The friction is scheduling: a caller that polls the signal descriptor in its
run loop and only wants to react to a *new* resize still has to call `size()`
to observe anything, and `size()` re-drains the descriptor. The poll result
and the drain then overlap, which makes the poll feel redundant. There is no
way to ask "has the size changed since I last looked, without draining
further".

## Guide-level explanation

A caller that already waits on the descriptor can ask whether a resize is
outstanding and only pay for the re-query when one is:

```rust
if let Some(size) = driver.size_if_notified() {
    frame.resize(size);
}
```

`size()` keeps its current behavior unchanged for callers that want the cached
value unconditionally.

## Reference-level explanation

```rust
impl TerminalDriver {
    /// Returns the terminal size if a resize notification was pending.
    ///
    /// Drains the signal descriptor; when at least one notification was
    /// received, re-queries the terminal, updates the cache, and returns
    /// `Some(size)`. When no notification was pending, returns `None` and
    /// leaves the cache untouched.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal size cannot be re-queried after a
    /// resize notification.
    pub fn size_if_notified(&mut self) -> io::Result<Option<Size>>;
}
```

- Implementation is the existing `size()` body with the `notified` flag
  returned instead of discarded: return `None` when `notified` is `false`.
- `size()` can be expressed in terms of it (`self.size_if_notified()` then fall
  back to the cache), or the two can share a private helper; either way the
  observable behavior of `size()` does not change.
- `&mut self` is required, as with `size()`, because the descriptor is drained.

## Drawbacks

- **More surface for a small win.** This adds a second size accessor that
  differs from the first only in what it does when nothing is pending. A reader
  has to know both to pick one.
- **Low urgency.** The current `size()` is correct; this is scheduling
  ergonomics, not a defect.

## Rationale and alternatives

- **Alternative: make `size()` return `None` instead.** Rejected: it would
  break callers that want the cached size unconditionally, and `size()`
  returning a plain `Size` is the more natural default.
- **Alternative: add a `poll_resize()` / `resized() -> bool`.** Rejected: a
  boolean plus a separate `size()` call is the same information split across
  two calls, and the common case wants the new size, not just the fact.
- **Alternative: `&self` size via a cached value only.** Rejected: this is what
  the cache already is; the point of the new method is to drain and re-query
  lazily, which needs `&mut self`.
- **Do nothing.** The situation is that a poll plus a `size()` call double up on
  the descriptor. It is not wrong, only redundant; leaving it costs a caller
  the ability to react to resizes without an extra `size()` call.

## Unresolved questions

- Is `size_if_notified` the clearest name, or should it be `poll_size` (
  echoing `poll` semantics) or `size_on_resize`?
- Should the notification-draining behavior be documented as a scheduling hint
  that callers may ignore, to make clear that `size()` remains the canonical
  query?

## Future possibilities

- If tuinix ever grows an event-loop helper, this method is the primitive that
  such a loop would call when it observes readiness on the signal descriptor.
