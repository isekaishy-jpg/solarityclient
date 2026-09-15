# CPU architecture cutover status

The user selected a direct cutover. The rollback tag is
`rollback/pre-cpu-cutover`, at `3be425958fb641aff014e218121a2c0b2e86b802`.
A later committed checkpoint is also preserved as `rollback/cpu-cutover-26cd4928`
(`26cd4928545b1aefb3059d539bb30806c08b7bec`). Neither tag contains uncommitted work.
The complete requirements remain in the [frame-job design](cpu-frame-job-design.md),
[composition design](cpu-crate-composition-design.md), and
[cache/residency design](resource-cache-residency-design.md).

## Connected source changes

- Replaced both Rayon pools with one persistent protected/flexible worker set;
  the configured total thread count is preserved and its resolved split logged.
- Added worker startup handshakes and x86-64 MXCSR control matching. Existing
  kernel arithmetic and stock reciprocal estimates remain unchanged.
- Added reusable owned frame batches, incremental job publication, independent
  result consumption, panic-safe state return and durable condition predicates.
- Removed the borrowed synchronous frame API. Unit poses now overlap subsequent
  admission/traversal; geometry starts during ordered placement traversal.
- Shared immutable M2 GPU source generations and retained particle/ribbon draw
  templates remove renderer registry borrows from geometry kernels. Effect
  simulation state and final transparent/publication order retain their owners.
- Added controlled tests for independent readiness, incremental producer restart,
  bounded batch admission, state return on worker/consumer panic, shutdown and
  rejecting a worker join that would deadlock its lane.
- Connected durable CPU completion notifications to a Windows event/timer/message
  wait bridge. Frame pacing, minimized service and cinematic deadlines use it;
  SDL remains the input owner and gameplay keeps its existing ordered cutoff.
  Native failures are explicit, producer faults are latched, and signal ownership
  survives late producers. SDL watches precede queue insertion, so the adapter
  retains a finite maintenance rescan (at most 16 ms before scheduling delays).
- Terrain streaming now installs the current camera window before offering a
  decoded tile for GPU admission. Regression coverage rejects both an unpolled
  completion and an already-staged old-window tile after movement.
- Replaced bare frame-result indices with typed, owner-checked epoch handles.
  Frame batches reserve their node/edge metadata before input transfer and can
  append backward-only dependencies. Completing workers directly release ready
  successors; domain failure, panic and cancellation suppress dependent kernels
  while retaining all owned inputs. Nonblocking outcome/consumption is available.
- M2 geometry publishes each completed result in traversal order while later
  kernels can still run. Effect-state reclamation follows publication on success
  and every error path. Packet-dependent receiver callbacks retain their ordered
  barrier; broad visibility alone does not authorize a receiver query.
- Geometry metadata reservation uses selected frame work plus the already-queued
  effect tail. It does not allocate job cells for all distant resident models.
  Dependency execution and output relocation now have focused folder modules.
- Added reusable bounded completion ports and external producer ownership. Typed
  phases can depend on another batch's completion without parking a worker.
  Subscription cancellation affects only its consumer; abandoned producers and
  failed prerequisites preserve waiting inputs and report dependency failure.
- The executor now tracks admitted frame epochs for shutdown. Open producers are
  closed and unresolved external gates are cancelled before waiting for drain;
  finite running work retains its state until completion. Phase publication is
  part of drain, so a port cannot be recycled while delivery is still active.
- Executor drop follows the same shutdown order. Batch disposal closes its
  producer without blocking a worker on another kernel; dispatch retains owned
  inputs and executor admission until terminal publication. Explicit reclamation
  remains the path for callers that need their inputs back.
- M2 scene-light evaluation is a separate owned CPU phase after packet-dependent
  receiver queries, gated by geometry completion. It overlaps transparent ordering
  and transfers its vectors without cloning the spatial bank. Ordered Rc light
  ownership and callback selection stay on main; all vectors return before errors
  propagate or the renderer borrows them.
- Frame phases now reserve all external prerequisites transactionally and run
  only after every producer succeeds. A failed input releases the phase's other
  subscriptions; a rejected reservation releases every earlier edge. Different
  typed producer phases compose through their existing readiness identities.
- Added reusable validated frame graph templates, with compact independent
  phases and flat backward-only dependency edges. Binding reserves the whole
  phase before input transfer and retains typed, generation-checked results.
  Existing owned pose batches and the retained scene-light phase use this path.
- Added prerequisite urgency and propagation through unresolved phase/resource
  readiness. Blocking result consumers and phase reclamation raise urgency;
  pose and scene-light phases declare it at admission. Queued runners move to
  the urgent bucket, and ordinary runners yield at kernel boundaries. Promotion
  uses reserved metadata work rather than recursive calls and pins its epoch
  until propagation ends. Execution eligibility remains unchanged.
- The flexible worker alternates queued background service and frame boundaries.
  Protected workers still cannot execute the general background adapter. Ready
  queue capacity now covers the admitted phase/runner bounds at pool creation.
  Dispatch startup, queue policy and worker parking have separate folder modules.
- Phase readiness emits its own coordinator notification after durable terminal
  publication, including empty phases whose last work is priority metadata.

- Added executor-wide byte admission for retained frame job cells, dependency
  metadata, graph topology, ready/priority queues, shutdown registry and readiness
  subscribers. Growth reserves old plus replacement capacity before input moves;
  clearing/reclaiming keeps retained capacity charged. Rebinding to another
  executor transfers charges and preserves allocation identity until actual growth.
- Added typed result pages and immutable shared leases with one charge per
  allocation. Exclusive reclaim, failed transfer and final-consumer disposal have
  pressure/lifetime coverage. Domains must account for allocations nested in a
  result separately; sharing a page does not recursively discover its heap graph.
- Runtime config supplies separate frame/required/speculative byte allowances.
  F10 detail snapshots report usage, ownership categories, peaks and limits with
  one ledger sample only on marked detail frames. See the
  [storage contracts](cpu-storage-contracts.md) for exact scope and configuration.

- Connected retained model-local mesh/particle/ribbon packet and vertex/index
  buffers, particle sort indices and copied palette overrides to the execution
  ledger. Each model reserves output before transferring placement simulation
  state. Workers receive fixed-capacity writers; pressure returns an error without
  implicitly allocating a larger destination. Charges follow the buffers through
  worker ownership, ordered publication, reuse and retirement.
- Cached immutable authored particle-capacity bounds with each shared GPU source,
  using the existing native-evidenced rate/lifetime headroom calculation. This
  covers a new simulation whose current pool is still zero. It reserves output
  capacity only; live simulation growth, emission and final GPU stream-capacity
  reporting continue to use their existing stock rules. Capacity rules now have a
  separate module under the particle simulation folder.
- Removed the production inline geometry path. World and glue/login M2 presentation
  both receive the application executor. Reference-only scene evaluation moved
  into the test tree. Particle presentation uses an in-place sort with explicit
  input-ordinal ties, retaining equal-depth ordering without stable-sort scratch.

## Still required for the complete cutover

- Typed shared-result leases across domains and main-only continuations.
  Templates, heterogeneous phase fan-in and frame urgency propagation now exist; resource
  cache/I/O integration still requires its complete concrete dependency graphs.
- Calibrated cost buckets and straggler reporting, plus required/speculative/
  retirement priority in the concrete background services. External producers
  expose urgency, but those services must still consume it. Frame urgency is
  monotonic within an epoch; live cache-consumer priority withdrawal is not yet wired.
- Connect reservations to allocations nested inside domain job state and the
  complete required phase working set. Executor-wide scheduler metadata and typed
  result-page accounting now exist; model output and override buffers now adopt it. Live simulation/pose storage,
  final frame streams, ordinary asset buffers and caches still require adoption,
  connected working-set admission, explicit trimming and maintenance policy.
- Extend the native bridge to loading/GPU-slot waits and main-ready continuations.
  Current frame consumers still wait at their necessary consumption boundaries.
- Cross-domain terrain/WMO/UI/rendering overlap and phase-specific M2 demand;
  ordered receiver lighting and end-of-frame state reclamation still have barriers.
- Shared asset request identity/lifecycle, stock-evidenced animation demand and
  retention, derived cache invalidation, byte-budgeted residency and GPU retirement
  from the resource design.
- Full causal wait/queue attribution, overhead/scaling checks, and matched
  movement/loading/live measurements. No FPS gain is established by compilation
  or synthetic correctness tests.

These are remaining implementation requirements, not optional deferred scope.
No numbered Testing build has been produced from this in-progress cutover.

## Checkpoint validation

### Model output admission checkpoint

The full workspace suite passed 1,464 tests with 33 ignored. Workspace Clippy
and formatting checks passed. The motion fixture compares meshes, particle/ribbon
streams and simulation state, lighting, shadows and ordering with the frozen
serial path, and verifies cleanup after an intentional resource mismatch. It now
also observes real output charges during successful frames and their release on
retirement/disposal. The particle fixture checks fixed-writer parity, equal-depth
presentation order, unchanged admission usage and full-destination rejection.
The new CPU buffer test covers non-growing writes and charged capacity after drain.

Logs are in ignored `target/cpu-domain-workspace-tests.log` and
`target/cpu-domain-clippy.log`. No new numbered package or live performance result
is established. Stock simulation pools and final frame streams are not yet charged
by this model-output integration.

### Retained execution storage checkpoint

The full workspace suite passed 1,463 tests with 33 ignored. Workspace Clippy
and formatting checks passed. The 54 CPU tests include new pressure, old-plus-new
growth, exclusive/shared ownership, cross-executor transfer and shutdown lifetime
coverage. The warmed graph/fan-in fixture still records zero allocator calls for
1,000 activations with prerequisite promotion enabled. Runtime configuration
coverage checks defaults, per-class overrides, zero speculative byte allowance,
malformed counts and duplicate options.

Logs are in ignored `target/cpu-storage-workspace-tests.log` and
`target/cpu-storage-clippy.log`. These establish correctness and warmed scheduler
allocation behavior, not domain-wide byte coverage, live frame-time improvement
or a new numbered Testing package.

### Frame priority and service checkpoint

On 2026-09-15, 1,453 workspace tests passed with 33 ignored, including M2
motion/visibility/receiver-light parity under the changed scheduler. A subsequent
review added notification after terminal phase publication. Final formatting,
workspace Clippy and all 46 CPU tests passed after that correction. The new
empty-phase fixture checks notification after priority metadata finishes. The
allocation fixture now enables prerequisite priority on every activation and
still observes zero allocator calls for 1,000 warmed bindings.

Controlled queue tests prove inherited priority ahead of older ordinary work,
yielding after a running kernel, propagation through a pending phase chain,
reset on epoch reuse and alternating single-worker background/frame service.
Logs are in ignored `target/cpu-priority-workspace-tests.log`,
`target/cpu-priority-final-tests.log` and `target/cpu-priority-clippy-final.log`.
The full workspace run preceded the final notification correction; its focused
CPU coverage and final Clippy are recorded separately. No live frame-time gain
or numbered Testing build is established by this checkpoint.

### Reusable graph and fan-in checkpoint

On 2026-09-15, the full workspace suite passed 1,447 tests with 33 ignored,
including the existing moving M2 and receiver-light parity checks. Final
formatting and workspace Clippy passed. All 40 CPU tests passed after adding a
separate allocation-counting fixture: 1,000 warmed diamond-graph activations with
two external prerequisites made zero allocator calls on the coordinator/workers.
That fixture measures fixed scheduler metadata reuse, not nested domain buffers
or the whole client's allocation rate. It was added after the full workspace run;
production source was unchanged during the final focused validation.

Logs are in ignored `target/cpu-graph-workspace-tests.log`,
`target/cpu-graph-final-tests.log` and `target/cpu-graph-clippy-final.log`.
Coverage also checks multi-input failure/capacity rollback, different typed
producer phases, template validation, repeated epoch binding and stale handles.

### Phase-readiness and lifecycle checkpoint

The phase-readiness/scene-lighting checkpoint passed the full workspace suite
with 1,439 tests and 33 ignored on 2026-09-15. This includes moving M2 geometry,
receiver-uniform and light-source-order parity. A subsequent worker-disposal
review removed the destructor wait; all 32 CPU tests then passed, including its
new controlled in-flight disposal regression. Final formatting and workspace
Clippy passed. The full workspace suite preceded that final disposal change;
the complete CPU suite covers it. Logs are in ignored
`target/cpu-readiness-workspace-tests-final.log`,
`target/cpu-readiness-lifecycle-tests.log` and
`target/cpu-readiness-clippy-final.log`.

Readiness coverage includes late registration, subscriber capacity reuse,
abandoned producers, heterogeneous phase ordering, chained failure, and both
explicit shutdown and executor drop with unresolved gates/open producers.
These checks establish ownership and ordering, not a live performance gain.

### Bounded-dependency checkpoint

The bounded-dependency/ordered-publication checkpoint passed the full workspace
suite with 1,432 tests and 33 ignored on 2026-09-15. The moving M2 parity fixture
was then extended to alternate scene lighting and compare receiver uniforms and
light-source order; its focused rerun passed. Existing coverage also checks
particle/ribbon recovery after a geometry error. Logs are in ignored
`target/cpu-dependency-workspace-tests.log`,
`target/cpu-dependency-lighting-parity.log` and `target/cpu-dependency-clippy.log`.
These are correctness checks, not a measured frame-time improvement.

### Native-wakeup checkpoint

The native-wakeup/current-window checkpoint passed formatting, workspace Clippy
and all 1,427 tests, with 33 ignored, on 2026-09-15. Coverage adds native arm and
notification races, late-producer lifetime, SDL event preservation, already-seen
Windows input, latched native failure, frame-limiter failure propagation and
old-window rejection before GPU admission. Logs are in ignored
`target/cpu-wakeup-workspace-tests.log` and `target/cpu-wakeup-clippy.log`.

### Initial owned-batch checkpoint

On 2026-09-15, formatting and workspace Clippy passed. The full
`cargo test --workspace --all-features` run passed 1,417 tests with 33 ignored.
This includes pose parity across camera/override changes and geometry parity
against serial traversal during motion, visibility changes and effect-state
cleanup. Both geometry paths use the retained draw-template constructor;
renderer validation and stock rendering fixtures cover its packet behavior.
The initial full run exhausted disk space while linking; after removing only
generated debug incremental caches, the complete rerun passed with
`CARGO_INCREMENTAL=0`. Logs remain in ignored `target/cpu-cutover-tests.log`
and `target/cpu-cutover-clippy.log`. These checks establish correctness coverage,
not completion of the remaining architecture or a measured performance gain.
