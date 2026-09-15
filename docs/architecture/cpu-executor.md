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
order before reporting terminal failure. Executor shutdown closes tracked open
producers and cancels unresolved external gates before waiting for frame drain.
An epoch registration includes its generation, so an old executor cannot stop a
batch that has subsequently rebound elsewhere. Running cancellation is
acknowledged after the finite kernel returns; it never steals mutable state from
a running worker. This is not cooperative preemption inside domain kernels.

M2 pose and geometry consumers use these checked identities. Geometry publication
consumes the ordered completed prefix before final reclamation, so stream copies
can overlap later kernels. Receiver-light callbacks still follow actual emitted
packet demand; their ordering is not inferred from broad visibility. Cross-batch
dependencies now include phase completion ports and multi-producer fan-in.
Main-only continuation dispatch, background service priority and global byte budgets
remain required work.

## External readiness and phase edges

`CompletionPort` owns a reusable generation with an explicit subscriber maximum.
`CompletionProducer` is a single external completion owner; abandoning it cancels
that generation. `ReadyToken` names the generation without owning its resource
payload. A conflicting second outcome or a late completion after reuse is rejected.
Reset requires terminal publication, finished delivery and released subscriptions.
The domain still owns the decoded asset, typed shared product or GPU lease; a
notification never substitutes for that payload or for GPU completion.

`FrameBatch::begin_after` reserves the complete supplied prerequisite list before
admitting any owned input. `begin_when` uses the same path with one prerequisite.
Duplicate resource generations are rejected. Partial reservation failure releases
all earlier subscriptions, and input failure cancels only this phase's remaining
edges. Each callback carries its epoch and input slot, so late/duplicate delivery
cannot decrement a new phase's readiness count.
The whole phase remains parked in metadata while its worker lanes serve other
work. Completion racing subscription binding is delivered exactly once. The
producer releases the port lock before delivery; the sole nested lock order is
batch metadata -> port metadata for unsubscribe. Delivery changes readiness and
enqueues workers, never runs domain kernels inline. Even an empty/failed closed
phase publishes through a scheduled runner when released by a prerequisite,
preventing recursive failure propagation across long chains.

Dropping a consumer removes only its subscription and closes its producer.
It does not wait inside a worker destructor: queued/running dispatch records
retain the owned state, and executor admission remains live until completion.
Callers needing their inputs back explicitly reclaim before disposal. Both
explicit executor shutdown and executor drop close admission, stop tracked epochs
and cancel unresolved external gates, then drain finite running tasks.
Reclamation waits through terminal port delivery before recycling
the epoch. Readiness ports visit actual bound subscribers during completion;
they do not scan the maximum subscriber allowance for each completed frame.

M2 geometry exports a phase token. Once its actual packets establish receiver
demand, scene-light evaluation consumes that readiness and owns the light/receiver
vectors on a worker while main sorts transparent packets. The token is normally
already complete at that point; the useful overlap is light evaluation versus
transparent ordering, not a claim that receiver callbacks run before geometry.
The existing ordered light-source owner remains on main, and every vector returns
before the renderer accesses the resulting scene bank. Resource-cache consumers
and I/O producers are still integration work.

## Reusable graph structure

`FrameGraphTemplate` holds immutable, validated topology. Registered operations
remain on `FrameBatch<T>`, which also owns reusable activation/result storage.
This separates the design's graph template and binding lifetimes without erasing
payload types or allocating a closure per activation. `with_dependencies`
validates backward-only, nonduplicate edges once; `independent` describes a root
phase without allocating a record for every model. Admission still reserves the
exact current input count, not all resident scene owners.

`start_graph` binds all inputs and optional external prerequisites transactionally.
Wrong input counts and admission failure preserve the caller's vector. The
complete connected phase is installed before its ready nodes are dispatched.
Activation visits only those nodes/edges; template storage may then be reused or
discarded because dependency counters and result handles belong to the binding.
Different operation types compose as phases through readiness ports, while local
nodes use the registered typed kernel. `start` uses the same independent-template
path, and M2 lighting retains its template across frames.

These APIs do not yet reserve a multi-phase application's aggregate byte working
set or publish domain-owned shared product leases. Those
remain explicit cutover requirements rather than implicit properties of a DAG.

## Urgency and service boundaries

`FramePriority::Prerequisite` marks a known critical phase. Blocking result
consumption and reclamation also call `require_urgent`; a phase's urgency rises
at most once during its epoch. The scheduler moves already queued runners into
the urgent FIFO bucket. Queue push reads atomic urgency while holding the queue
lock, so a racing launch cannot strand an urgent runner in the ordinary bucket.
Running kernels are not preempted. Their runner yields between kernels when
priority metadata, more urgent frame work, or flexible-lane background service
needs progress. Logical result/publication order remains domain-owned.

Promotion traverses unresolved readiness dependencies using one queued metadata
record per promoted phase. It holds admission until propagation completes,
validates producer generations and calls other owners outside phase/port locks.
Long chains therefore consume queue entries, not recursive stack frames. Queue
capacity is reserved from the admitted phase count and worker count at startup.
Background closures never become eligible on protected workers through urgency.
The flexible worker alternates queued background service and frame boundaries;
an executing bulk call still runs to its existing finite completion boundary.

`CompletionProducer::is_urgent` exposes demand to an external resource owner.
Its concrete required/speculative/retirement queues still need integration;
this flag alone is not a background priority policy. Urgency is monotonic for a
frame epoch and resets on reuse. Cache consumer withdrawal and calibrated cost
buckets remain required work. Pose and scene lighting now declare their known
prerequisite urgency; other frame phases can be promoted by actual consumption.

Terminal phase publication sends a coordinator notification after readiness is
durable. Job-result notifications alone cannot cover empty phases or metadata
that finishes after the last kernel. The notification runs outside scheduler
locks and retains the existing native coalescing behavior.

## Validation

External tests cover result ownership, bounded admission, reserved interactive
capacity, shutdown rejection, draining after result-handle disposal,
task-panic recovery, and non-consuming completion observation:

```powershell
cargo test -p solarity-cpu --all-targets
```
