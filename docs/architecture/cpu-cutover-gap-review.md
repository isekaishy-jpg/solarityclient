# CPU cutover gap review, 2026-09-20

This records the user's implementation review and camera-smoothness regression
against Build 159, source `8a4ae681`. The findings below were checked against
source, rather than inferred from the design vocabulary. They supplement the
existing three design documents; the complete cutover remains unfinished.

Subsequent implementation connects current placement ownership, explicit worker
policy and the first real `JobContext` consumers. See the
[adoption record](cpu-cutover-foundation-adoption.md) for the current contracts
and limits. The findings below retain the original Build 159 review baseline.

## Camera smoothness: resolved in the user's Build 161 test

On 2026-09-20 the user confirmed no crash and smooth camera motion again after
the Build 160 camera-clock correction and Build 161 output-retention fix. This
closes the reported visual regression based on live observation. It does not
establish which contribution dominated or close the remaining performance work.
The investigation below records the original report and evidence limits.

The user describes camera movement that previously looked smooth and clear but
now looks rough, blurred, possibly choppy or tearing. That report is broader
than an FPS counter or a confirmed horizontal tear. Preserve it as an open
visual/pacing regression, not as an already diagnosed camera-solver failure.

The [Build 159 F10 review](testing-build159-performance.md) establishes irregular
world frame times, including 32 ms frames with expensive residency and unit-state
updates. Those stalls can explain rough motion, but do not prove the cause of
the reported blur or actual display tearing.

Verified boundaries:

- `client.rs` still admits gameplay input before world service and presentation.
  `platform/sdl_platform/wait.rs` pumps native events while waiting but leaves
  them queued for the next gameplay cutoff; it does not mutate the camera in
  already prepared packets.
- Camera collision/resolution in `client_services/world_camera.rs` remains on
  main. Its per-service-frame cache predates the CPU cutover (commit `581da609`).
  That file, player camera math, the main client loop and local pose update did
  not change between the compared Build 155 and Build 159 sources.
- Vulkan present-mode selection and the relevant environment/screen-effect
  preparation paths also did not change between those sources. The Testing
  installer still selects `gxVSync=0`, as it did before these builds. Uncapped
  mode prefers IMMEDIATE, then MAILBOX, with the existing supported-mode policy.
  This identifies the requested policy, not a measured display/scanout cadence.
- M2 preparation and its worker ownership changed during that interval. An
  unchanged camera solver does not rule out a timing, state-coherency or renderer
  regression elsewhere in the frame.

The existing capture does not tie input timestamps, resolved camera identity,
rendered camera identity, screen-effect mode/blur parameters and actual display
cadence into one observation. Its GPU screen-effect duration is not the effect's
selected parameters, and queue submission time is not displayed-frame time.

Required closure: compare equivalent moving views with the same Soap scene and
settings, separate ordinary/detail/off capture behavior, establish input/camera/
submission identity and active screen effects, then correlate display cadence
with CPU stalls. Keep gameplay input, camera-dependent culling, shadows and
render packets on the same state boundary. Do not move camera mutation later,
change VSync, disable effects or reuse old scene products merely to hide the
symptom. A measured FPS average or a hidden numerical replay alone does not
close the user's visible regression.

### Earlier camera sharing: missing clock dependency

The broader history review found that `581da609` combined terrain-demand and
presentation camera resolution through a per-service-frame cache. Its key checked
pose, collision settings, terrain revision, extent, far clip and pivot pitch,
but skipped the client clock. A cache hit returned before
`sample_camera_collision(now)`. Stock `603D30` collision-height recovery depends
on that clock even when the pre-sample pose and all spatial providers match.
Unchanged camera files between Builds 155 and 159 do not establish that this
earlier optimization was correct.

The immediate cache return now requires exact client-tick equality as well as
matching providers. When streaming/UI work crossed a tick, presentation first
samples current collision recovery, then checks spatial reuse against that
updated pose. An unchanged sample keeps the expensive collision result; a
changed sample triggers a new solve. This avoids blindly repeating the roughly
0.4 ms camera solve seen in the Build 159 world capture. The time stored is the
sample's input time, never a later completion time. The existing frame reset and
geometry/input invalidation remain in place. This restores the clock dependency
without changing ordered mouse admission, worker ownership, the stock recovery
curve, VSync or screen-effect policy.

The regression fixture keeps transform/view/scene fixed, advances the clock
between terrain and presentation, and verifies both rejection of the old camera
and advancement of the real Systems recovery/projection. Additional cases cover
clock wrap, same-tick provider changes and reuse after an unchanged later sample.
This establishes a cache-validity bug,
not that this bug accounts for the entire reported free-look blur or pacing
regression. The matched visible-motion comparison remains required.

## Missing JobContext

`JobContext` appears in the composition design but is absent from the CPU crate.
`FrameBatch` currently accepts `fn(&mut T)` or `fn(&mut T) -> JobOutcome`;
`LoadBatch` uses the latter. The worker restores trace context around execution,
but the kernel has no executor-provided context argument. Cancellation marks
running nodes under the batch lock and acknowledges it after kernel return;
it does not expose a cooperative cancellation check to that kernel.

The existing byte reservations, buffers, result pages and tracing machinery
should support the context. An empty facade type would not complete this work.
The context needs:

- Stable job/epoch provenance and optional diagnostics inherited from admission.
- Scoped scratch charged to the admitted working set, with explicit ownership
  transfer for results that outlive the call.
- Cooperative cancellation/withdrawal observation at valid domain boundaries,
  with owned state returned on success, failure, cancellation and abandonment.
- A finite-step service contract for resumable kernels. Cancellation or urgency
  cannot interrupt an arbitrary decoder, skip required gameplay updates or allow
  partial RNG/animation advancement to be silently discarded.

It must not grant access to runtime/SDL, mutable application state, global RNG,
arbitrary blocking services or worker-side dependency waits. Connect real frame
and loading consumers and cover reuse, withdrawal, cleanup, warmed allocation
and disabled-instrumentation overhead before declaring the context implemented.

## Findings from the pasted review

| Area | Verified implementation | Required completion |
| --- | --- | --- |
| Worker policy | `dispatch/startup.rs` makes only the last worker flexible; `executor.rs::background_worker_count` returns one. `CpuPoolConfig` has total count, task capacity and storage limits. | Runtime resolves an explicit validated protected/flexible/bulk plan; CPU enforces and reports it. Bulk work stays inside the whole-process concurrency/admission budget. |
| Dispatch overhead | `Core::launch` pushes runner records individually. Each push locks `Mutex<Queues>` and calls `notify_all`. Claim/completion use the batch mutex. | Batch eligible runner insertion under one queue lock and wake after publication. Measure dispatch-lock and ownership-lock waits, frame enqueue-to-start percentiles and cold-worker wake latency, including 1/3/5/7-worker cases. |
| Typed products | Homogeneous batches, generation-checked readiness and shared `CpuResultLease<T>` pages exist. Readiness is not intrinsically paired with each cross-domain typed product. | Keep homogeneous kernels; bind product identity, readiness, generation and retained payload ownership together so consumers cannot pair a valid wakeup with an obsolete product. Connect concrete resource/domain consumers. |
| Memory | Metadata, result pages and selected domain allocations use the ledger. Many nested buffers, simulation/pose state, caches and retained generations do not. | Complete working-set admission and ownership/retirement accounting. Report covered and uncovered categories; the ledger is not a total client-memory bound. |
| Service fairness | Required loading and retirement alternate queue turns. Steps can yield, but indivisible calls can occupy the sole flexible worker for milliseconds. | Calibrate finite service steps and isolate unavoidable blocking/bulk calls behind explicitly bounded eligibility and admission. Turn fairness alone is not a latency guarantee. |

The review's 128/256/64 MiB defaults match runtime's frame/required/speculative
allowances. Increasing those limits does not account for allocations outside the
ledger. `CpuResultLease<T>` is useful existing storage, not a finished typed
cross-domain product contract. Ordinary service `cpu.job.queue_wait` timing also
exists; that does not replace the missing frame-runner/lock/wake measurements.

Two policy distinctions remain deliberate:

- Protected workers must not run ordinary potentially blocking decoders or
  large destruction merely because they were idle when dispatched. Any borrowing
  needs an explicitly safe, bounded eligibility contract; otherwise the next
  frame inherits an unpreemptible delay.
- Capability discovery and a resolved worker split belong to this cutover.
  Fixed affinity, NUMA placement and hot pool resizing were excluded from its
  initial implementation. Advisory topology must distinguish unknown information
  from known hardware without claiming a logical CPU is a dedicated physical core.

Keep the central scheduler until measurements justify a replacement. The current
live evidence establishes substantial serial main-thread work and spare worker
capacity; it does not establish queue mutex contention as the dominant cause.

## Completion order and evidence

Treat the camera report as a regression to close alongside removal of the
confirmed repeated placement searches. Complete JobContext and the explicit
execution plan before treating further domain migrations as proof that the CPU
foundation is complete. Build typed product bindings on the existing readiness
and result storage; finish nested memory accounting and bounded service adoption
through representative consumers. Measure queue changes and whole-frame scaling
instead of inferring savings from thread count or API shape.

This review adds explicit missing requirements to the cutover status. It does
not implement the context, change scheduler policy, diagnose tearing conclusively
or claim that the camera regression is fixed.
