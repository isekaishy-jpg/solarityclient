# CPU service execution and owned skeletal storage

The shared executor now separates execution eligibility from consumer urgency.
`CpuService` still describes required, retirement and speculative demand.
`CpuServiceExecution` describes finite nonblocking work versus indivisible bulk
work. Promoting a request cannot turn a decoder into protected frame work.

## Connected behavior

- Runtime resolves half the configured compute workers as flexible (minimum one),
  one reserved service turn, and a bulk allowance equal to flexible capacity.
  Explicit protected/flexible/service/bulk configuration remains authoritative.
  Four compute workers therefore resolve to two protected and two flexible
  workers; all four can execute frame kernels. No hidden compute pool is added.
- Independent loading DAG nodes can run on multiple flexible workers. The previous
  one-runner restriction is removed. Service queue storage reserves the complete
  runner bound before admission. Demand changes move every queued runner of the
  same loading epoch while preserving their ownership and queue age.
- Finite services can run when the bulk allowance is occupied. Service eligibility
  uses a retained count, so frame publication does not scan a loading backlog.
  Ground-detail chunk preparation uses finite execution with resident inputs;
  archive, codec, appearance and arbitrary cleanup calls remain bulk work.
- All remaining production M2 pose, spatial and lighting batches receive
  `JobContext`. Cold appearance, GameObject, Glue, audio, screenshot, shared request
  and resumable archive consumers now explicitly receive their admitted context.
  Required ownership return still completes after demand withdrawal. Context
  diagnostics inherit the admitted trace and do not read clocks while disabled.
- Foreign service calls restore the coordinator's captured numeric controls before
  publishing results. A second restore covers discarded-result destruction before
  the worker takes another operation. Panicking discarded-result destructors are
  contained rather than killing a persistent worker and stranding its bulk count.
- Skeletal transforms, local matrices and sequence-clock capacity now carry a
  unique CPU reservation with the owned pose. Geometry and root-pose admission
  reserve complete model capacity before dispatch. Moving or swapping a pose
  moves its charge; allocation refusal preserves the prior pose. Deferred palettes
  keep model-compatible storage instead of exchanging unrelated scratch capacity.
- Shadow command recording feeds sparse successful measurements back into its
  job cost hints. Scheduling order can change; GPU command submission order cannot.
- Minimap archive ownership transfers only after CPU admission succeeds. Its
  folder-backed loader opens one archive or decodes one requested texture per turn
  and returns the mounted reader for reuse. Admission refusal no longer drops it.
- Startup reports architecture, usable SSE2/AVX2, usable logical concurrency,
  compute split, network workers, main coordinator and GPU completion ownership.
  Audio/recording backend ownership is distinguished from the compute allowance.
  No physical-core, affinity or NUMA placement claim is made.

## Boundaries and validation

The implementation preserves the existing stock ordering of animation clocks,
global random consumption, callbacks, attachment publication and camera sampling.
Loading concurrency changes independent owned operations, not gameplay publication.
The configured CPU byte limits still describe adopted allocations, not total RSS:
ordinary asset/cache storage and live effect state are not all charged by this change.
Bulk calls are isolated within the configured allowance, not made preemptible.

Focused tests exercise three concurrent loading nodes, complete runner ownership
through promotion/withdrawal classes, finite progress with saturated bulk capacity,
foreign numeric-state restoration, discarded-result cleanup, scalable default
policy, skeletal charge transfer and transactional storage refusal.
Formatting and workspace Clippy (all targets/features, warnings denied) pass.
All 1,654 workspace tests pass across 103 suites: zero failed, 33 existing ignored.
No frame-time gain is inferred from compilation.

One sequential baseline/candidate pair completed 4,096 frames using Soap and
300 equipped NPC fixtures at `(1515.34, -4417.27, 18.0499)`, 2560 x 1440,
`gxVSync=0`, `extShadowQuality=5`, with identical cameras and source inputs.
The baseline is the preserved Build 171 source benchmark. The candidate SHA-256
is `83BB1496A1C56F6E5DC361C0AB379277426FCA3921C17A70A5091CD2664BFD97`.
No compilation or GPU tests overlapped either run.

| Phase | Baseline median ms | Candidate median ms | Baseline p95 ms | Candidate p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Streaming | 23.915 | 24.571 | 30.424 | 33.635 |
| Stationary | 25.165 | 24.307 | 28.216 | 25.799 |
| Orbit | 17.176 | 16.740 | 28.401 | 27.111 |
| Pointer camera | 24.958 | 24.890 | 26.796 | 27.104 |

These are hidden-window diagnostic frames, not live desktop FPS. The fixture
uses installed terrain, FrameXML and Vulkan, but excludes network, the movement
solver, audio and overlays. A single pair does not isolate scheduler gains from
presentation variance. Resident tile counts and final bone counts match across
the pair; time-dependent mesh counts differ by up to three in the stationary
views. The largest candidate frame is its first: 343.864 ms versus 98.715 ms
for baseline, mostly inside presentation. Do not hide that cold tail behind
the lower stationary median or attribute it to a specific change without evidence.

A separate candidate travel run completed another 896 frames, including
`(-80, 15, 0)` travel out/back and settled reentry. No crash or CPU storage refusal
occurred. Travel medians were 21.2–21.3 ms, settled median 24.162 ms; the first
frame was 98.435 ms. This qualifies ownership through the fixture's movement,
not live camera smoothness or the complete movement/worker-scaling matrix.
Raw artifacts are ignored `target/cpu-complete-equipped-comparison-*` and
`target/cpu-complete-travel-comparison-*`.

This change connects execution and ownership boundaries. It does not complete the
remaining main M2 admission, cross-resource source-request authority, total working
set admission, native GPU acquire/growth servicing or the broader cache/residency
requirements in [the cutover status](cpu-cutover-status.md#still-required-for-the-complete-cutover).
