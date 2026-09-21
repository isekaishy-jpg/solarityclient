# Discovered loading dependencies

This continues the CPU cutover after Build 173, alongside parallel world command recording. Remaining main-thread admission, cross-resource authority and whole-working-set requirements are tracked in the cutover status.

## Owned suspension

`CpuTaskPermit::submit_resumable_with_context` accepts `CpuTaskStep::{Continue, Wait, Complete}`. `Wait` consumes an already reserved `CpuTaskDependency`. The executor retains the same closure, admission lease and result channel in a pre-admitted parked slot. It releases the physical worker and bulk allowance. Readiness resumes the operation on its current service FIFO; there is no polling loop, nested worker wait, new task or extra thread.

Each slot has a serial so cancellation and reuse cannot accept an old delivery. Reservation and binding use the existing durable completion port: publication before binding is replayed. Failed and abandoned sources also resume the domain, which consumes its original typed result. Cancellation wakes only the affected consumer. Shutdown closes suspension before waiting for admission to drain, so a still-owned external producer cannot strand shutdown. Cleanup and capture destruction remain worker work.

Optional dependency demand follows the waiting task's live service class, preserving other consumers' independent interests. Queue locks are released before forwarding demand. Source graphs must remain acyclic and an operation must never suspend on its own producer. Dependency state stays inside the existing service allocation, leaving the shared frame queue record compact. Existing bounded-turn `ControlFlow` submissions adapt to the same implementation. F10 records `cpu.job.dependency_to_resume` separately from execution; it includes dependency latency and subsequent ready-queue delay, and disabled instrumentation takes no clock reading. `JobContext` identity and worker scratch ownership remain unchanged.

## Connected terrain consumers

Runtime entry, prewarm and neighbor streaming use the resumable interface. Referenced MDDF sources now join the namespace M2 request authority, including sources already being prepared by another consumer. A new producer decodes within its existing bulk turn. An existing producer supplies a typed M2 result and suspension edge. The MDDF cursor cannot advance past that dependency or publish a partial tile.

WMO preparation yields between MODF registrations and selected MODD owners. Nested MODD M2 requests use the same shared authority for both ADT and global-WMO scenes. The synchronous path drives the same registration stages with its explicit local cache owner. Stock first-reference MODF order (`0x007C6150`), selected group-referenced MODD membership (`0x007BF740`), duplicate validation, placement transforms, collision/liquid registration and whole-generation publication are preserved. No additional gameplay fallback is introduced.

Terrain preparation and WMO residency are decomposed into folder modules. The shared-WMO and ground-detail extensions below connect their source authority. Remaining appearance/effect consumers still require conversion.

## Evidence and validation

The fresh pre-change hidden profile of the 300-NPC fixture contained 2,032 ordinary frames and 16 detailed frames. The two ordinary M2 admission scope sites sum to approximately 5.54 ms per frame. World command recording averaged approximately 3.13 ms; queue present averaged approximately 22.59 ms. These are inclusive profiled timings from that hidden run, not matched user FPS measurements. The loading change does not claim to remove the M2 main-thread cost or the presentation driver time.

Focused scheduler tests exercise single-worker forward progress, early completion, repeated dependencies, failure, cancellation on both sides of suspension, shutdown, dropped consumers, resumed panic and live shared-producer demand. The broader CPU suite, including allocation-free graph activation and scratch reuse, passed after correcting the worker identity integration. Runtime compilation and the four archive-backed terrain tests pass, including real shared-source success, abandonment and withdrawal. The suspension/terrain source passed all 1,666 workspace tests, with 33 existing ignored, and the optimized 896-frame movement/loading replay. The subsequent renderer source passed 1,668 tests. The complete source including shared WMO, ground detail, bounded root/group decoding and cancellation-safe producer retirement passed all 1,676 workspace tests (zero failures, 33 ignored), formatting and workspace Clippy with warnings denied. The optimized replay qualification below records no overall performance gain. [Build 174](testing-build174-continuations.md) packages and installs this source with verified executable identity and hash.

## Shared WMO extension

Catalog clones now carry the same WMO request authority. Exact namespace/root
identity selects one producer for a complete root and all numbered groups;
joined consumers retain the original success or error through typed CPU
readiness. M2 and WMO use a common readiness bridge while retaining separate
cache and decoder policies. No negative cache or guessed retention age is added.

ADT/global-WMO MODF loading now suspends for shared roots before advancing the
registration cursor. GameObject WMO loading yields between the root and each
selected default-set MODD owner, sharing nested M2 results. The runtime callback
and GPU placement publish only after the whole source is ready. Cancellation
returns the owned archive bank and releases partial inputs on the worker.
Synchronous diagnostic consumers retain their explicit synchronous entry point.

Final source release coalesces a maintenance bit. The existing retirement service
now drains WMO sources in bounded groups outside metadata locks; capacity refusal
retains the pending notice. Live leases prevent collection. Root/group decoding
now yields after the root, after each independently resolved numbered group, and
before final validation. The original order of reads and errors remains intact:
all groups load before cross-group fog and spatial validation. Only the completed
model enters shared residency. Individual archive decompression and one group
decode remain indivisible codec work; ordinary asset/cache byte accounting is
still an open requirement.

The strict reader is decomposed into root, group, layout and preparation modules.
Its synchronous diagnostic entry point drives the same cursor to completion.
The new source test checks unpublished root/group stages, complete publication,
and abandonment after root decode. Runtime terrain and GameObject continuations
own the producer between turns. Withdrawing its initiating scene drains only
that already-owned source in bounded steps, preserving joined consumers; it
does not construct further scene resources. A consumer waiting on another
producer instead releases its own interest immediately. A controlled runtime
test cancels after root decode, checks the joined root's complete publication,
and verifies that the cancelled scene never starts its doodad requests. Source
failure or unexpected producer destruction still publishes a terminal error.

## Ground-detail extension

Ground-detail asset preparation is now a cursor over the same sorted unique
GroundEffect model identifiers. It yields between definitions and joins shared
M2 readiness before constructing each immutable detail mesh and texture provider.
The tile stage retains that cursor and cannot publish a partial provider bank.
Synchronous tile loading drives the same cursor with its explicit local source
owner. Existing scatter, density, RNG and placement code is unchanged.
The new archive-backed test holds a shared producer, verifies progress of other
work on one worker, and then checks complete model/texture publication.

The final combined source passed all 1,676 tests with zero failures and 33
ignored, workspace Clippy and formatting. The optimized candidate completed
1,792 crowded frames across 1/2/4/8 workers and another 896-frame F10 run.
The paired earlier binary also completed. Measurements and their limitations
are recorded in [world recording](cpu-world-recording.md#combined-source-qualification);
the pair does not demonstrate an overall performance improvement.
