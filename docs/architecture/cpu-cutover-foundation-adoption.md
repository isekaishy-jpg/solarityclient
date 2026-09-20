# Current ownership, execution policy and job context

This checkpoint connects three existing cutover requirements. It does not finish
the CPU cutover or claim a measured frame-rate improvement.

## Placement ownership before render publication

`M2PlacementStorage` now owns a dynamic-owner index alongside its structural
journal. Append, compaction, extraction, effect ordering and in-place retirement
update that index before a consumer can inspect it. Repeated owners retain native
order through intrusive links in the existing lineage records. Matching an owner
visits only that family; rebuilding the index visits dynamic records only.

Creature/player generation checks, movement/animation owner lookup and retained
component/effect lookup use this current simulation identity. Dirty render
metadata no longer selects a full-scene per-unit search. Unit removal also uses
dynamic membership and set membership for retained/removed GUIDs. Unit residency,
removal and state updates are separate children of `m2/units`.

No animation/RNG order, source-generation comparison or visibility predicate
changes. Regression fixtures compare duplicate-owner lookup with native order
through mutations before render publication. The earlier 9.499 ms residency and
8.917 ms unit-state spike motivates this boundary; those totals included other
work and are not claimed savings from the change.

## Explicit execution plan

`CpuPoolConfig` requires a validated `CpuExecutionPlan`. Runtime selects protected
and flexible counts, a required-service reserve, and a concurrent bulk/service
limit. CPU does not derive a hidden split. Every service call, including existing
indivisible calls, consumes flexible capacity under the shared bulk limit.
Protected workers never execute that work. Reserved flexible workers prefer
required/retirement turns; remaining flexible workers prefer frames and assist
service when no frame is ready. A one-worker plan still alternates eligible work.

Runtime options are `--cpu-flexible-workers`, `--cpu-service-workers` and
`--cpu-bulk-limit`, all defaulting to one in the initial runtime policy. Their
relationship is `1 <= service <= bulk <= flexible <= --cpu-workers`; the
remainder is protected capacity. Existing Testing launch arguments therefore
retain their prior split. Startup reports all resolved counts. No hidden worker
pool, affinity, NUMA placement, hot resize or adaptive tuning is introduced.

Configuration and scheduler fixtures exercise invalid counts, multiple service
workers and frame progress while bulk calls occupy their limit. A limit provides
concurrency isolation, not a maximum duration for an indivisible decoder. Finite
service-step adoption and whole-process scaling measurements remain required.

## Connected JobContext

`FrameBatch::with_context` and `LoadBatch::with_context` supply batch-local
epoch/index provenance, inherited diagnostics, atomic cooperative cancellation
and scoped typed scratch. Cancellation cells use one reusable, charged metadata
allocation per contextual batch, not an allocation per kernel invocation.
Running kernels hold its generation until they return, before terminal
publication permits another activation.

`CpuScratch<T>` is explicitly admitted before input transfer. A context opens a
non-growing writer whose guard clears values on normal return and unwind.
Allocation ownership and its byte charge remain with the admitted domain state.
Scratch is not a result lease and cannot publish borrowed temporary storage.
This initial typed storage is retained per domain job; it does not yet implement
generic worker-local arenas or account for all nested domain allocations.

M2 geometry groups now receive the context. Particle ordering uses its scratch
loan and group diagnostics inherit admission provenance. Living model simulation
finishes coherently even after consumer withdrawal. Dependent appearance and
GameObject loading observe cancellation before taking their mounted bank; later
indivisible work retains its existing completion/cleanup contract. Context does
not expose runtime state, RNG, dependency waits or implicit scratch growth.

Tests cover epoch reuse, stable retained charges, temporary destruction on
success/cancellation/panic, cooperative cancellation while running, and owned
input recovery. Broader context adoption into resumable service continuations,
worker-local scratch policy and optimized observer/dispatch overhead measurement
remain required. Typed cross-domain product binding is a separate remaining
requirement; diagnostic `JobIdentity` is explicitly not such a product token.

## Validation

Formatting and full workspace Clippy (all targets/features, `-D warnings`) pass.
Full workspace tests pass: 1,611 passed, zero failed, 33 existing ignored tests.
This includes the new owner-index, execution-plan and context regressions, the
moving serial/worker geometry comparison, and existing equipment, retirement,
mount, vehicle and loading ownership tests. These establish behavior and ownership
contracts; optimized overhead/scaling and matched live frame costs remain open.

A hidden debug replay completed 896 frames across streaming, stationary, orbit,
pointer movement, travel out/back and settling, using real Orgrimmar assets and
48 authored NPCs. The seven detailed samples reported a maximum retained geometry
Frame charge of 12,580,610 bytes and Result charge of 4,698,136 bytes. Capture
reported zero dropped samples and zero capacity overflows. These are sampled
retained charges, not whole-process memory or global peaks. This fixture omits
network, movement solving, audio and overlays; its debug timings are not a live
FPS comparison or evidence of an optimized speedup.
