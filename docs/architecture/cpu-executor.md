# CPU executor ownership

The CPU executor is the second implementation boundary after deterministic
asset resolution. It owns finite CPU-intensive work such as archive decoding,
format parsing, visibility preparation, and later simulation batches. Network
and timer I/O remain on the separately owned Tokio runtime.

## Stock evidence

The fingerprinted build-12340 executable contains `W32/SThread.cpp` references
in three recovered functions:

| Function | Observed responsibility |
| --- | --- |
| `0x007700a0` | Creates a named Win32 thread and records its ID and handle in a process thread registry. |
| `0x0076ff30` | Runs the supplied entry point, then removes the completed thread from that registry. |
| `0x00770290` | Starts a process and optionally creates a tracked waiter thread. |

This evidence establishes explicit creation, identity, and completion
ownership. It does not establish that stock used a work-stealing task pool.
Solarity adapts the ownership requirement to a private Rayon pool because its
64-bit architecture decomposes expensive loaders and systems into smaller
parallel work units.

## Executor contract

- Worker count and maximum in-flight work are required configuration values;
  the CPU crate does not guess a machine policy.
- The in-flight bound covers running and queued work. Submission returns an
  immediate typed capacity error instead of blocking the interactive producer
  or growing an unbounded queue.
- Every admitted operation returns a single-owner completion handle. Dropping
  the handle discards only the result; executor shutdown still owns and drains
  the work.
- Task panics are converted into a typed completion failure and release their
  admission slot. They do not destroy the remaining private pool.
- Shutdown closes admission transactionally, waits for every admitted task,
  then drops the Rayon pool. `Drop` performs the same drain as a final safety
  boundary, while normal composition code must call `shutdown` so errors remain
  observable.
- Callers must submit finite structured operations. A task must not start
  detached Rayon work of its own; nested parallel iterators are acceptable
  only when they complete before the submitted operation returns.

The shared lifecycle mutex is touched only for admission, snapshot, completion,
and shutdown. Task bodies and result transport do not hold it, so expensive HD
asset work cannot serialize on the accounting boundary.

## Validation

External tests cover result ownership, bounded admission, shutdown rejection,
draining after result-handle disposal, task-panic recovery, and non-consuming
completion observation:

```powershell
cargo test -p solarity-cpu --all-targets
```
