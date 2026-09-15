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
  Incremental phases declare node/edge maxima before dispatch; known independent
  phases reserve their exact node count. Excess work is rejected before ownership
  transfer. Aggregate bytes and nested domain allocations remain an outstanding
  storage contract recorded in the cutover status.
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

## Frame dependencies and consumption

`FrameBatch<T>` registers its kernel once. `begin` reserves a `FrameBatchPlan`;
`push_after` accepts only earlier handles from the same batch and generation.
Flat edge records and a reserved propagation queue avoid per-node allocations
and recursive failure walks. Completing workers release successors directly,
including parallel fan-out; main does not discover readiness by polling parents.
Registration and terminal-parent observation use the same synchronization boundary.
Nodes with a failed/cancelled predecessor retain their state without executing.

`FrameJob<T>` is a non-owning typed slot/generation handle. Its weak owner prevents
allocation-identity reuse from admitting a foreign batch; checked epoch advance
rejects old-frame reuse. `outcome` and `try_with_result` do not wait. `with_result`
waits for only its own node and rejects unfinished worker-side waits. Consumption
leases state outside the scheduler lock and returns it even if the consumer
unwinds. Domain error payloads remain in T and are recovered through reclamation.

`close` ends incremental admission; `reclaim` returns every input in admission
order before reporting terminal failure. An open producer must close or reclaim
its phase before asking the executor to shut down. Running cancellation is
acknowledged after the finite kernel returns; it never steals mutable state from
a running worker. This is not cooperative preemption inside domain kernels.

M2 pose and geometry consumers use these checked identities. Geometry publication
consumes the ordered completed prefix before final reclamation, so stream copies
can overlap later kernels. Receiver-light callbacks still follow actual emitted
packet demand; their ordering is not inferred from broad visibility. Cross-batch
dependencies, external readiness and main-only continuation dispatch remain
required work, as do reusable dependency templates and global byte budgets.

## Validation

External tests cover result ownership, bounded admission, reserved interactive
capacity, shutdown rejection, draining after result-handle disposal,
task-panic recovery, and non-consuming completion observation:

```powershell
cargo test -p solarity-cpu --all-targets
```
