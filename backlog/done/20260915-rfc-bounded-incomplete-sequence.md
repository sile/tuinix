# RFC: Drop the buffer trim instead of adding a recovery path

- Status: accepted

## Summary

`InputDecoder::trim_buffered_bytes(max_len)` bounds the buffer by discarding
bytes from the front, so when the buffer holds one long incomplete sequence it
cuts that sequence in half. Earlier drafts of this RFC tried to make that cut
safer — abandon the sequence at a boundary the decoder understands, or resync
after the junk. This RFC reaches the opposite conclusion: the decoder should not
offer a way to shed buffered bytes at all. The input is either well formed, in
which case nothing accumulates, or the source is broken, in which case there is
nothing worth recovering and the application should fail rather than paper over
it.

## Motivation

The decoder holds bytes for one reason: the bytes so far might still become an
`Input`. Every place that holds is a case where more bytes would let the parser
finish, and the fixes up to now removed the places where that was never true (a
complete sequence held forever). What is left is honest incompleteness: a
sequence whose terminator has not arrived.

There is no bound on how long such a sequence can be. A line of noise, a mis-sent
control sequence, or a stream from a program that is not speaking the protocol
can all open one, and nothing in the bytes that follow has to close it. A caller
that reads from a device it does not control needs an answer for it.

The answer today is `trim_buffered_bytes(MAX)`, and it is worth asking what
problem it actually solves.

The buffer only grows when the bytes fed so far are the prefix of one sequence
that has not ended. Which sequences can do that is a short list:

- `ESC [` (or `0x9b`) followed by nothing but digits and semicolons, so the
  parameter run never reaches a terminator.
- A control-string body whose terminator never arrives (`ESC ]`, `ESC P`,
  `ESC _`, or their C1 introducers).
- A truncated UTF-8 lead byte or mouse report that never gets its continuation.

Of these the control-string body is the realistic one, because a body is
arbitrary length: an APC carrying an image, or an OSC report from a program that
forgot to close it, can be megabytes. The parameter run needs a source that
emits only digits and semicolons, which no working terminal does.

So there are really two situations behind an oversized buffer, and they want
different answers:

1. **The decoder is wrong.** The sequence is well formed and terminated, but
   tuinix fails to see the terminator and holds it forever. The fix is in the
   decoder (the one-byte `ST` in a C1 control string is one such case), and no
   amount of trimming helps because the bytes will keep arriving.
2. **The source is wrong.** A program is emitting junk, or a sequence was cut
   off and will never finish. There is no correct output to recover: no decoder
   can tell where the next real sequence starts, because the bytes that would
tell it apart are exactly the bytes that never came.

`trim_buffered_bytes` addresses neither. It turns case 1 into a silent
behavior: the application trims and the sequence is lost, so the missing
guarantee stays missing. For case 2 it replaces "an unbounded buffer" with "a
buffer of unknown correctness": the bytes left after the cut have lost their
sequence context, so what the decoder reports afterward is not what arrived.

The argument for dropping it is that a TUI does not need the robustness a server
does. A server accepts input from attackers, so it must bound and recover. A TUI
reads its own terminal, or the pty its child is attached to. If that stream is
broken, the honest response is to stop, not to keep decoding a stream that no
longer means anything. `buffered_bytes()` already gives the application the one
fact it needs to make that call.

## Guide-level explanation

`InputDecoder` shrinks to two methods for this concern: `feed()` and `next()` as
the way in, `buffered_bytes()` as the way to observe how much is held. There is
no method to drop bytes, and `trim_buffered_bytes` is removed.

An application that can receive a broken stream checks the bound itself and
treats exceeding it as a fatal condition:

```rust
loop {
    // ... feed, drain, draw ...
    if decoder.buffered_bytes() > MAX_BUFFERED_BYTES {
        eprintln!("input does not look like terminal input; giving up");
        std::process::exit(1);
    }
}
```

The application still owns the policy — what bound, what to do — which is where
it belongs. What the decoder stops doing is offering a halfway measure that
looks like a recovery and is not one.

## Reference-level explanation

`InputDecoder::trim_buffered_bytes()` is deleted. Nothing takes its place: no
`abandon_incomplete_input()`, no `reset()`, no bound inside the decoder.

`buffered_bytes()` stays, unchanged. It is an observation, not a policy — it
reports how many bytes are held and takes no view on what should happen next.
The value of the pair is that the application can see the condition and decide;
the value of removing `trim` is that the decoder's answer to the condition is
only "tell me the bytes", never "here is a way to keep going".

The invariant this leaves is the one the recent fixes were all reaching for: `
next()` returns `None` only while a well-formed sequence is still arriving, and
every well-formed sequence eventually settles. Under that invariant an oversized
buffer is always a statement about the input, not about the decoder.

The distinction from `commit_escape()` is why this is a removal rather than a
rename. `commit_escape()` settles an *interpretation* the application has
reasoned about (a lone `ESC` was Escape, not the start of a sequence), which is
a decision only the application can make and the decoder correctly leaves to it.
Dropping bytes is not an interpretation; it is giving up on the stream, and the
decoder has no privileged knowledge that makes its version of that better than
the application's.

## Drawbacks

- An application that wants to keep reading through a broken stream has no
  supported way to do it. That is deliberate, but it is a real loss of a choice
  the old API allowed. The application can still build a fresh `InputDecoder`
  and lose everything, but it cannot keep the bound.
- `buffered_bytes()` becomes the only signal, so an application must poll it
  somewhere in its loop. This is a small burden, and one an application that
  cares about the bound is already paying today to decide when to trim.
- Removing a public method is a breaking change. tuinix is pre-1.0 and the
  project has decided breaking changes are acceptable when the API is wrong, so
  this is noted rather than used as an argument against.

## Rationale and alternatives

**Abandon the sequence at a boundary the decoder knows**
(`abandon_incomplete_input()`, the earlier draft of this RFC). Better than a
byte offset — the decoder does know where the held sequence starts — but it
still ships a method whose whole purpose is to keep a partially received stream
going, and it silently discards a sequence that might merely have been slow. It
also answers a question the application has to answer anyway: after calling it,
when does the application decide the stream is broken for good? Every answer to
that question is the same check on `buffered_bytes()` this RFC proposes doing
directly.

**Resynchronize after the junk** (the decoder, told to give up, consumes until
it sees a byte that can start a sequence). Rejected: there is no correct choice
of resynchronization point. `ESC` is wrong because a control-string body may
contain it; a newline is wrong because a body may contain that too. Any choice
is a guess about where the sender stopped being broken, and the decoder is not
in a better position to guess than the application.

**Keep `trim_buffered_bytes` and document the hazard better.** Rejected for the
reason the earlier draft gave: the hazard is not a misunderstanding a doc can
prevent. The argument is a byte count, and a byte count that lands inside a
sequence is the operation working as designed. But the deeper problem is that
improving the docs leaves the method in place, and the method is the thing that
suggests a broken stream is worth limping along with.

**Have the decoder hold a bound** (`with_max_buffered_bytes(n)`). Rejected, as
it was in the trim RFC: a bound is a policy with a user-visible trade-off (a
real sequence gets cut), and a foundational layer must not hide that decision.
`buffered_bytes()` plus an application-side policy keeps the decision where it
belongs.

**Keep the method and only rename it or fix the docs.** Does not address the
point; the method's existence is the problem.

## Unresolved questions

None. The scope is a removal: delete `trim_buffered_bytes`, keep
`buffered_bytes()`, and close out the earlier trim RFC as superseded.

## Future possibilities

- If a real application turns out to need to keep reading through a broken
  stream, that is new information and deserves its own RFC with the concrete
  case attached. Nothing here forecloses it; the answer today is that no such
  case is known.
- The one stream that does accumulate legitimately — a control string that is
  genuinely still arriving — is bounded by its terminator, not by a byte count,
  provided the decoder recognizes the terminator. The fix for the one-byte `ST`
  is filed separately, and is the reason to look at case 1 before adding escape
  hatches for case 2.
- Whether the trim RFC's original motivation should be corrected in its history.
  Investigation while writing this found that pastes do not grow the buffer: a
  paste is decoded character by character and the buffer stays small, and an
  OSC/DCS/APC body is likewise consumed as it arrives. The buffer grows only for
  an unterminated parameter run or an unterminated control string.

## Outcome

Implemented in [#39](https://github.com/sile/tuinix/pull/39) (merged as `1dadb60`).

Implemented in [#39](https://github.com/sile/tuinix/pull/39). `trim_buffered_bytes`
is gone from `InputDecoder`; what remains is `feed()`, `next()`, and
`buffered_bytes()` as an observation. `examples/demo.rs` no longer trims — it
prints a message and exits when the buffer exceeds its threshold, which is the
pattern the Guide-level section shows. The trim RFC is now marked superseded, and
`docs/input-decoding.md` describes an oversized buffer as a statement about the
input rather than a condition to recover from.

The scope is unchanged from what is described above.
