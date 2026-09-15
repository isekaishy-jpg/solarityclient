# CPU executor ownership

The direct cutover replaces the Rayon pools with one persistent worker set.
[Cutover status](cpu-cutover-status.md) separates connected implementation from
remaining requirements of the [frame-job design](cpu-frame-job-design.md),
[CPU composition design](cpu-crate-composition-design.md), and
[cache/residency design](resource-cache-residency-design.md).

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
Solarity adapts the ownership requirement to persistent owned workers because its
64-bit architecture decomposes expensive loaders and systems into smaller
parallel work units.

## Executor contract

- Worker count and maximum in-flight work are required configuration values;
  the CPU crate does not guess a machine policy.
- The in-flight bound covers running and queued work. Submission returns an
  immediate typed capacity error instead of blocking the interactive producer
  or growing an unbounded queue.
- One configured worker is flexible: it services admitted background work,
  then helps frame work. The remaining workers are protected from background
  operations. The single-worker plan uses its sole flexible worker.
- Speculative admission stops while any background work is in flight. Required
  background submission retains the configured hard admission bound.
- Frame batches have separate bounded admission and reusable typed state.
  Pose outputs are independently consumed; geometry inputs are published while
  ordered traversal continues. The old synchronous frame API is removed.
  Batch admission currently bounds epochs, not node counts or total bytes;
  the outstanding storage contract is recorded in the cutover status.
- Every admitted operation returns a single-owner completion handle. Dropping
  the handle discards only the result; executor shutdown still owns and drains
  the work.
- Task panics are converted into a typed completion failure and release their
  admission slot. They do not destroy the remaining private pool.
- Shutdown closes admission transactionally, waits for every admitted task,
  then joins every worker handle. `Drop` performs the same drain as a final safety
  boundary, while normal composition code must call `shutdown` so errors remain
  observable.
- Callers must submit finite structured operations. A worker cannot join an
  unfinished `CpuTask`; dependencies must return control to the scheduler.
- Startup waits for each worker's initialization handshake. On x86-64 it
  matches the coordinator's MXCSR controls, preserving the existing numeric
  kernels and the unrefined stock reciprocal estimate.
- Runtime supplies a `CoordinatorNotifier` before admission. Tasks and frame
  batches publish durable outputs before calling it outside scheduler/result
  locks. The notifier must be bounded and non-panicking; runtime latches native
  failures for main-thread reporting. Notifications carry no result payload,
  may coalesce, and keep their native signal alive through producer teardown.

The shared lifecycle mutex is touched only for admission, snapshot, completion,
and shutdown. Task bodies and result transport do not hold it, so expensive HD
asset work cannot serialize on the accounting boundary.

## Validation

External tests cover result ownership, bounded admission, reserved interactive
capacity, shutdown rejection, draining after result-handle disposal,
task-panic recovery, and non-consuming completion observation:

```powershell
cargo test -p solarity-cpu --all-targets
```
