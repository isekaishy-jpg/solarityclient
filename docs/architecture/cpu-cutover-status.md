# CPU architecture cutover status

The user selected a direct cutover. The rollback tag is
`rollback/pre-cpu-cutover`, at `3be425958fb641aff014e218121a2c0b2e86b802`.
A later committed checkpoint is also preserved as `rollback/cpu-cutover-26cd4928`
(`26cd4928545b1aefb3059d539bb30806c08b7bec`), along with
`rollback/cpu-cutover-b677ed91` (`b677ed912e15fd6dd2c88e71a14c81db5d3fa8a9`).
A further checkpoint is preserved as `rollback/cpu-cutover-b49ab5ae`
(`b49ab5ae2d1c63e3272b584990e0a8ecdbc1d05a`).
Further checkpoints are `rollback/cpu-cutover-c007aaf8`
(`c007aaf814c4df21b6e655e1ffc0cdd131e5950e`) and
`rollback/cpu-cutover-0cfa2a2a` (`0cfa2a2a57854c3a5fac61a6bd4a7f26184135ec`).
The latest requested checkpoint is `rollback/cpu-cutover-9127ce4a`
(`9127ce4a788c44278a991e8cfa28dfdeba2e0dbe`), pushed before resuming the cutover.
These tags contain committed work only.
The complete requirements remain in the [frame-job design](cpu-frame-job-design.md),
[composition design](cpu-crate-composition-design.md), and
[cache/residency design](resource-cache-residency-design.md).

## Connected cutover changes

Current-clock bone demands discovered during ordered placement traversal now
use the existing CPU pose kernel through a saved placement continuation. Clock
selection, callbacks, RNG and event-window consumption run once; publication
resumes after the worker result is ready and its source, clock, view and overrides
match. Render consumers reuse a full worker palette; other CPU consumers request
only named bones. Existing native frame waits service the dependency, and the
reusable job releases its source after frame completion or abandonment.

Formatting and runtime Clippy with warnings denied pass. Runtime validation
passes 471 library tests and 68 stock-seed integration tests (28 existing ignores),
including held-worker suspension, sparse/full pose parity, animated offscreen
light result consumption and exactly-once placement accounting. Evidence is in
ignored `target/late-pose-final2.log`. Late dependencies remain ordered one at a
time; expired-variation event samples and callback-time sampling still run on
the main owner. Broader graph, memory and source cutover work and performance
qualification remain open. Build 174 remains installed; no FPS gain is claimed.

Retained M2 generations now charge the existing CPU Result allowances once for
their decoded geometry, primary SKIN, collision/hierarchy buffers and nested
animation, material, light, ribbon and particle tracks. Runtime binds the shared
source authority to its executor budget before model loading. Speculative
sources use their separate allowance; required consumers atomically transfer
the charge before consumption. Refusal preserves the original speculative
generation and allows retry. Cache-only retention keeps its charge until the
qualified release deadline, with no change to that policy.

The accounting measures owned capacities, including unused Vec slots. Shared
canonical path strings, cache/request metadata, allocator overhead and temporary
encoded/decode scratch remain outside this generation charge. Other source
domains and complete working-set admission remain open.

Validation exposed a startup stack overflow on the default test thread. The
process's large service owner is now boxed instead of being copied inline
through application startup results; the unchanged startup/shutdown test passes
without increasing the stack limit. Formatting, workspace Clippy with warnings
denied, all 1,686 workspace unit/integration tests (33 existing ignores) and six
doc tests pass. Logs are ignored `target/model-storage-final2.log` and the
preceding failure/reproduction logs. Build 174 remains installed; this source
has no measured FPS result yet.

The unit pose batch now includes mounted riders and vehicle descendants whose
transforms have already been resolved by ordered callbacks. Camera demand uses
the model's bounds; shadow demand uses its attachment root. Exact generation,
clock, transform and override checks still guard ordered result publication.
This extends the existing batch without another scheduler or an extra animation
tick. A moving-camera jump/landing test confirms worker-result consumption for
local-player, remote-player and creature mounts and bodies with unchanged RNG.
Offscreen inherited-shadow coverage verifies worker consumption during the
entry fade and shadow packets after the native opacity cutoff. Formatting,
runtime Clippy with warnings denied, and all 470 runtime tests pass (27 existing
ignored). Validation is in ignored `target/rider-pose-final2.log`; this source
has no measured FPS result yet.

Screenshot readback now retires through the configured GPU completion service,
allowing native input servicing while the existing device-idle barrier completes.
The renderer retains exclusive capture/submission ownership through that wait;
CPU encoder admission still precedes readback and saturated queues preserve the
original captured frame. Absent, unpresented and collected captures add no wait.
Offline rendering retains synchronous readback. Formatting and rendering/runtime
Clippy with warnings denied pass, as do the real GPU capture/resize test and both
runtime screenshot tests (quality, pixels, saturation and save failures). The log
is ignored `target/capture-wait-check.log`. This has no measured FPS result.

The root pose batch now also admits mount clocks already selected by ordered
callbacks and named-bone requests from offscreen units. Attachment, event and
light consumers receive typed `M2BoneSamples` from workers; visible and shadow
consumers retain full palettes. Request records and sparse output working storage
carry frame reservations. Publication checks the source generation, clock, view,
overrides and exact named demand before swapping the result into the ordered
owner. Sampling an event window leaves its consumption cursor untouched. The
offscreen equipped-NPC and moving-camera serial comparisons exercise this path;
attachment inputs discovered later in traversal are connected by the placement
continuation above; callback-time sampling remains to be connected.
Formatting, runtime Clippy with warnings denied, and all 468 runtime tests pass
(27 existing ignored). The three offscreen mount populations consume worker
samples while retaining callback/RNG behavior. Validation is recorded in ignored
`target/named-pose-final.log`; this source has no measured FPS result yet.

Sky models now join the namespace M2 authority in resumable worker tasks, including
stars and WMO-selected skies. The ordered main owner resolves aliases, first phase
flags and request timestamps without archive access; queue saturation retains the
request for later admission. Shared source waits release the worker, while model
textures and shader preparation run after readiness. Celestial textures decode
one per worker turn and publish as one bank. Glue servicing begins these process
resources before world presentation. Sky animation, RNG and GPU publication retain
their ordered main owner.

The sky source and rendering suite passes nine tests (four existing installed-data
tests ignored), including single-worker suspension/cancellation, shared generation
identity, capacity retry, alias flags, WMO replacement and hidden-scene clocks.
Formatting and runtime Clippy with warnings denied pass. The first test compile
exhausted disk space; clearing stale compiler intermediates allowed the rerun to
finish. Logs are in ignored `target/sky-cutover-{check,test,lint}.log`.

Appearance loading now carries player/NPC attachments, item visual children,
mounts and Glue pets through the namespace-wide shared M2 authority. One admitted
resumable task retains the archive bank and source leases across discovered
pending dependencies; a waiting consumer releases its worker and withdrawal
returns its bank without cancelling other source consumers. An already claimed
primary producer completes before cancellation is returned. Nested source records
and result pins use the Required storage budget. Derived construction runs once
after source readiness and consumes those shared generations without inserting
a second local retained copy. Missing authored attachment links still omit their
visual source. This replaces the separate primary-only loading-batch adapter;
whole-payload accounting, requested animation loading, and the remaining frame
cutover requirements below remain open.

Appearance validation passes formatting, workspace Clippy with warnings denied,
and 1,682 workspace unit/integration tests (33 ignored). The final priority
regression passes the 464-test runtime rerun, and workspace doc tests pass after
rebuilding missing shared-target artifacts. Logs are in ignored
`target/appearance-cutover-{final,priority-final}.log`. This source is not yet
installed and has no measured FPS result.

After Build 174, geometry working-set preparation moves from ordered M2 traversal
into the owned worker turn. Main still admits the request records, copied pose
inputs and shared sorting lanes. The worker then admits its palette, packet and
simulation buffers before advancing any effects; refusal returns unadvanced
state through the existing ordered error/reclamation path. This distinguishes
request admission from the numeric phase's working-set admission. It does not
claim a complete connected-frame reservation or remove the remaining main-owned
animation, attachment and receiver work. The moving-scene reference comparison,
including a real worker with a refused memory budget, passes. Formatting,
workspace Clippy with warnings denied, and all 1,676 workspace tests pass with
zero failures and 33 ignored. Logs are in ignored
`target/m2-worker-admission-{clippy,test}-all.*.log`. Build 174 remains installed;
this source has no measured FPS result yet.

[Testing Build 174](testing-build174-continuations.md) is installed. It connects
discovered-dependency suspension, shared terrain/GameObject/ground-detail sources,
bounded WMO decoding and parallel world command recording. All 1,676 workspace
tests pass with zero failures and 33 ignored, as do formatting and Clippy.
The paired hidden replay is slower overall; this package establishes neither
an FPS gain nor completion of the remaining cutover requirements.

[Effect storage and native GPU waits](cpu-effect-storage-and-gpu-waits.md) connect
particle/free-slot pools, fixed ribbon history and named-bone scratch to allocation
ownership. World/Glue, UI/loading and cinematic acquisition use the existing GPU
completion thread, with native servicing at world resource/quality barriers and
swapchain retirement. The current source retains stock simulation and camera order;
formatting, workspace Clippy and all 1,659 tests pass (zero failures, 33 existing
ignored). This does not complete the remaining architecture listed below.
[Testing Build 173](testing-build173-storage-waits.md) packages this source.

[Service execution and skeletal ownership](cpu-service-execution-cutover.md) now
separate finite/bulk eligibility from request urgency, remove the one-runner loading
graph restriction, and scale the default flexible capacity with configured compute
workers. Service queues reserve all runners and retain constant-time eligibility.
Remaining M2 batches and cold consumers receive JobContext; skeletal allocations
carry their budget through worker/main ownership transfer. Minimap loading preserves
its reader under admission backpressure and uses resumable archive/texture steps.
Shadow recording uses measured cost hints. Formatting, workspace Clippy and all
1,654 tests pass (zero failures, 33 existing ignored). No new performance gain or
full-cutover completion is claimed.

[Worker-produced effect streams](effect-stream-upload.md) now use their final
shader byte layout directly for bulk particle/ribbon/index upload. This removes
main-side per-element serialization without another worker phase, staging copy
or join. Formatting, Clippy and all 1,646 tests pass. Two alternating equipped
pairs show 3.118-5.518 ms lower matched stationary medians, but variable native
presentation prevents attributing that entire gain to CPU work. The separate
profile reduces upload by 0.591 ms and renderer thread cycles by about 11.1%,
while total profiled frame time is worse. This removes redundant consumer work;
main admission, presentation behavior and the complete cutover remain open.
[Testing Build 171](testing-build171-effects.md) packages this boundary.

[Owned M2 finalization](m2-owned-finalization.md) now transfers complete geometry
assembly and transparent ordering to a retained CPU operation using `JobContext`,
budgeted output capacity and compact stable ordering scratch. Main consumes owned
contiguous streams at the original receiver boundary; camera, animation/RNG and
renderer interfaces are preserved. All 1,642 tests, Clippy and formatting pass.
Four equipped comparisons show 0.892-0.967 ms lower matched stationary medians;
orbit medians are slightly higher and the lighter unarmed control regresses by
0.145 ms. Profiles move about 2.695 ms out of main publication, accompanied by
increased coordinator waiting. Main admission remains 3.853 ms and renderer CPU
4.572 ms. These remaining costs and lighter-scene overhead stay open.
[Testing Build 170](testing-build170-finalization.md) packages this boundary.
The [live Build 169 capture](live-build169-capture.md) still concentrates CPU
work on main and attributes 6.267 ms of its busiest 12.333 ms interval to M2
admission/publication. It overlapped compilation, limiting timing comparisons.

[Retained owner attachment inputs](m2-attachment-inputs.md) replace per-owner
scene-wide membership and parent-result scans with topology-owned request groups
and indexed ordered frame samples. Duplicate order, first-parent selection and
any-hidden-rider behavior are preserved through one publication API. Formatting,
workspace Clippy and all 1,638 tests pass. Four alternating 300-equipped-NPC runs
show 0.232-0.414 ms lower matched stationary medians, with mixed tails; the unarmed
control is essentially flat. Separate profiles reduce main M2 admission by
0.421 ms/frame but leave 4.099 ms admission and 3.001 ms publication, plus
4.655 ms renderer CPU time. Those remaining large boundaries take priority over
further small lookup changes. [Testing Build 169](testing-build169-attachments.md)
packages this checkpoint; it does not complete main-thread distribution.

[Admitted worker scratch](cpu-worker-scratch.md) now carries physical execution
identity through frame/loading jobs and both service adapters. M2 particle sorting
uses versioned, budgeted worker lanes instead of a temporary container per model.
Admission and trimming preserve in-flight versions; scoped loans clear on return
and unwind. Formatting, workspace Clippy and all 1,634 tests pass; warmed graph
activation with scratch allocates nothing across 1,000 measured activations.
Four alternating comparisons show a small 0.036-0.069 ms increase in matched
stationary medians, slightly lower orbit medians and no demonstrated FPS gain.
Separate profiles show effectively unchanged combined M2 admission/publication
and about 26 KB less mean retained Frame-ledger memory in this fixture. The
change establishes required storage ownership; the linked report retains the
timing limits. [Testing Build 168](testing-build168-scratch.md) packages this boundary.

M2 geometry now transfers [stable owned job cells](cpu-owned-job-cells.md) through
staging, workers, consumption and reclamation instead of moving complete model
records. Cells remain budgeted during main-side retention and support transactional
executor rebinding. The existing generation/demand reuse policy and contiguous
renderer outputs remain authoritative. Formatting, Clippy and all 1,628 workspace
tests pass. Four alternating optimized runs show matched stationary medians
0.17-0.33 ms lower and lower camera-motion medians; isolated long frames remain.
A separate profile reduces main admission/publication from 2.641 to 2.413 ms per
ordinary frame without a renderer increase. This is a modest measured boundary
improvement, not completion of main-thread distribution or the requested multi-ms
target. [Testing Build 167](testing-build167-cells.md) packages this boundary;
the linked report records memory and fixture limits.

A [four-variant borrowed-output experiment](m2-borrowed-output-pages.md) also
failed to establish a useful whole-frame gain. Direct worker pages saved about
0.28 ms in publication but added about 0.25 ms in renderer consumption, including
after replacing per-packet callbacks with typed storage. All prototype changes
were removed before the stable-cell work; none was installed in Testing.
This rejects that particular output boundary, not the remaining cutover scope.

A [two-variant M2 output-assembly experiment](m2-output-assembly-experiment.md)
rejected a separate final-copy worker phase. Early overlap saved about 0.24 ms of
main publication/prefix work but added about 0.26 ms of phase-pending time; moving
camera results were mixed. Both prototypes were removed before the stable-cell
checkpoint. Future distribution must consume producer-owned output directly or
produce final records inside existing kernels, rather than adding another copy
phase and consumer join. The full remaining cutover scope below is unchanged.

Shadow command recording now uses the shared CPU executor with four exclusive
per-slot command pools, scoped unconditional joins and native input servicing.
Main still owns scene recording and the single ordered GPU submission. The
[ownership contract and controlled comparison](parallel-shadow-recording.md)
record 0.62-0.74 ms lower matched 192-NPC steady medians and the corresponding
CPU/GPU costs. Existing long frames and the requested multi-ms overall target
remain open; this does not complete M2 admission/publication distribution.

M2 palette publication now keeps completed worker pages through renderer upload,
removing the duplicate main-thread concatenation and scalar matrix packing.
[The ownership contract](m2-palette-pages.md) preserves ordered draw offsets,
shadow-only palettes and fenced GPU-slot lifetime. [Instruction sampling](m2-main-copy-evidence.md)
identified this transfer and further large draw-record copies; this boundary
alone does not complete main-thread preparation distribution.

A [controlled receiver-query experiment](m2-receiver-query-experiment.md) rejected
a late floor-only worker phase: matched steady frames did not improve, and
duplicated collision preparation introduced severe stalls. The prototype was
removed before Build 165. Future spatial distribution must
precede the first expensive consumer and share prepared immutable query inputs.

Typed immutable products now bind payload, readiness generation and lifetime in
the CPU crate. M2 dependent appearance/GameObject loading uses this boundary,
retaining source leases and consumer urgency without a separate request-slot
dependency. See [ownership, connected consumers and limits](cpu-shared-products.md).

The [Build 163 live Brewfest capture](live-build163-brewfest.md) records 11.254 ms
ordinary frames, including 5.432 ms across M2 admission/publication. Main consumes
0.964 of a core while each CPU worker consumes 0.075–0.084 cores. Scene changes
and incomplete background-activity isolation prevent a build-only regression
claim; remaining main-thread model work is a concrete cutover target.

An [isolated NPC-density sweep](npc-density-cutover-measurements.md) reproduces
roughly 4.5 ms of added frame cost at 192 authored NPCs. Preparation, publication
and Vulkan command recording all scale with population. This is a measured
remaining cost, not a performance fix or an exact live Brewfest reproduction.

`JobContext` now reaches finite and resumable services. Terrain retires obsolete
demand between whole operations and returns its owned archive bank without
publishing a partial resident. Required retirement and cache maintenance still
drain after consumer withdrawal. See [the service contract, tests and remaining
limits](cpu-service-context.md).

Runner publication now inserts a phase's reserved runners under one queue lock
and one notification. Sampled dispatcher/batch acquisition, queue residence and
useful native wake intervals distinguish those boundaries. The optimized
1/3/5/7-worker comparison exposes both scaling and empty-kernel contention; it
does not establish a live improvement. See the [measurements and exact interval
semantics](cpu-dispatch-publication.md).

The current installed checkpoint is [Testing Build 171](testing-build171-effects.md),
source `dae47563`, installed on 2026-09-20 at 17:35 EDT. All 1,646 workspace tests,
Clippy, formatting, optimized compilation and installed identity/hash checks pass.
Upload and renderer thread-cycle costs improve in the separate profile, but total
profiled frame time is worse. Full cutover requirements remain active.

The preceding checkpoint is [Testing Build 170](testing-build170-finalization.md),
source `bc57f191`, installed on 2026-09-20 at 16:44 EDT. All 1,642 workspace tests,
Clippy, formatting, optimized compilation and installed identity/hash checks pass.
The equipped fixture improves by about 0.9 ms, while unarmed and orbit medians
regress slightly. Full cutover requirements remain active.

The preceding [Testing Build 169](testing-build169-attachments.md),
source `75a21fca`, installed on 2026-09-20 at 15:55 EDT. All 1,638 workspace tests,
Clippy, formatting, optimized compilation and installed identity/hash checks pass.
The equipped comparison shows a modest median improvement with mixed tails;
large M2 publication and renderer costs and the complete cutover remain open.

The preceding [Testing Build 168](testing-build168-scratch.md),
source `5ae0207c`, installed on 2026-09-20 at 15:17 EDT. All 1,634 workspace tests,
Clippy, formatting, optimized compilation and installed identity/hash checks pass.
The scratch ownership contract and its small measured timing increase are recorded
above; this package does not establish an FPS gain or complete the cutover.

The preceding [Testing Build 167](testing-build167-cells.md),
source `e8c2c8eb`, installed on 2026-09-20 at 14:35 EDT. All 1,628 workspace tests,
Clippy, formatting, optimized compilation and installed identity/hash checks pass.
Its measured stable-cell improvement and remaining limits are recorded above.

The preceding [Testing Build 166](testing-build166-recording.md),
source `4fd3ab6d`. Full workspace tests passed 1,623 cases with 33 existing ignored;
the additional recording failure/unwind test also passes, alongside final Clippy
and formatting. Four unprofiled 192-NPC runs completed 16,384 frames: matched
steady medians improve by 0.62-0.74 ms against Build 165. Separate profiles show
main recording at 1.02 ms versus 1.59 ms, with sampled GPU time slightly higher.
Main M2 admission/publication still sum to roughly 2.67 ms in that fixture.
Long frames, matched live performance, the requested 5 ms overall reduction and
the remaining cutover requirements stay open. [Build 165](testing-build165-palettes.md)
records the earlier palette-page improvement separately.
The user also supplied a [modern Classic comparison binary](modern-classic-reference.md)
for architecture investigation; its identity is pinned, with no new disassembly
findings claimed yet.

- Simulation owner lookup now follows placement storage mutations before render
  metadata publication. Creature/player residency and state updates no longer
  scan the whole scene per owner while topology is dirty; duplicate ordering and
  in-place retirement remain explicit. Unit lifecycle code has focused children.
- Runtime resolves a validated `CpuExecutionPlan`, including protected/flexible
  counts, reserved service workers and concurrent bulk/service capacity. CPU
  enforces and reports the plan. Existing Testing defaults retain their split.
- `JobContext` now reaches M2 geometry and dependent appearance/GameObject loads:
  batch-local provenance, inherited diagnostics, atomic withdrawal and scoped
  preadmitted typed scratch. Particle sorting uses that scratch boundary, with
  unconditional temporary cleanup. See [implementation and remaining limits](cpu-cutover-foundation-adoption.md).

- Placement storage now journals dynamic membership and static layout changes.
  When no static identity or index changed, topology publication retains its
  static arrays, spatial partitions and WMO membership, refreshing only dynamic
  ancestry/owners and required-work membership. Actual static relocation,
  removal or replacement still selects complete publication.
- Effect partitioning visits only the suffix after the first effect. Replicated
  WMO updates preserve static keys and ordered first-owner light facts. Static
  visibility no longer carries unused source indices or walks its cache to remap
  them. F10 distinguishes retained static slots from republished fields; see
  [the publication contract and counter semantics](m2-topology-publication.md).

- Selected static M2 scenery now has an owned spatial-admission phase. Workers
  apply native camera, distance and independent shadow predicates to compact
  captured values; ordered traversal consumes a group once, without locking an
  executor result for each model. Static render bounds survive their residency.
  WMO visibility/opacity and shadow membership are captured after scene admission;
  hidden lights and offscreen casters retain their independent demands.
- Groups target 100 microseconds of calibrated work, capped at 64 candidates.
  Inline working sets, staging and references are charged before transfer. Errors
  remain at each model's original consumer, and abandoned frames reclaim every
  group. Dynamic/attachment admission and WMO scene queries remain ordered on main.
  See [the boundary and remaining limits](m2-static-admission.md).

- CPU result/phase wait scopes now cover only native condition waits and mutex
  reacquisition. Ready probes, callback/lease return and reclamation have separate
  meanings. Need links and one-based result owners retain the producing phase
  across frames; a constant-time reason identifies pending gate/node state.
  The result module separates waiting, consumption, reclamation and lifecycle.
- GPU host requests carry their origin to the dedicated completion thread.
  Host backend waits and exceptional drains are distinct from native coordination;
  ready readers dispatch no host request. Main pending/reader-check labels no
  longer imply native blocking. Win32 wait timing ends before signal validation
  and records the wake result. See [scope meanings and evidence](cpu-consumption-tracing.md).

- M2 render palettes and shadow-material packet construction now belong to the
  owned draw phase, including camera-culled shadow casters. Ordered CPU callbacks
  sample their named bones; an already prepared root palette can serve those
  callbacks directly. Shadow-only jobs neither take particle/ribbon ownership nor
  advance effect clocks. Static admission now uses the phase above. Main retains
  dynamic admission, attachment/callback order, RNG, receiver queries and final
  output order.
- Worker packets use local palette bases. Ordered publication selects only the
  palettes retained by the previous visible/shadow rules and relocates both mesh
  and shadow packets with checked arithmetic. The renderer now borrows those
  completed palette pages without concatenating them into a main-owned vector. Primary and environment shadow banks
  share the same sampled material packets. Worker failure and abandoned admission
  return every submitted or staged model's owned state.
- Calibrated small draw jobs share a finite scheduler group, targeting at most
  100 microseconds of estimated work and at most 16 models. Unknown or individually
  expensive work runs separately; a kernel is not forcibly preempted. Model-record
  capacity and shadow output buffers are charged before ownership transfer. F10
  model traces remain per model; `cpu.frame.execute` now measures a group for this
  phase, so per-call comparisons with prior builds are invalid. Numeric frame
  order and per-model successful calibration remain independent of dispatch order.
- The draw phase separates owned inputs/state, grouping, palettes, shadows,
  meshes, particles, ribbons and output publication. Shadow spatial admission and
  pure packet construction also have separate children. This removes more serial
  render work but does not complete M2 admission, broad topology publication,
  domain working-set accounting or the remaining cutover requirements below.

- Live local-player appearance now uses the shared primary-M2 request and owned
  population worker. Body texture composition, textures, attachments, mounts and
  immutable draw-template preparation run in that worker. A stable pending key
  skips rebuilding its main-side appearance plan. Source failure restores the
  cache bank; world/identity, texture-quality and appearance withdrawal reject
  obsolete output. Nested attachment/mount source requests still use local caches.
- Local and remote players share the same character construction function. Main
  keeps local scaling, the exact Glue-to-world transfer, animation callbacks and
  camera state. Publication applies current movement/view and preserves the current
  camera clock and mount transition. Existing residency continues its own motion
  until an exact replacement publishes. Table-derived collision dimensions are
  held independently of visual residency, so worker loading and texture-quality
  invalidation do not defer that metadata. The local folder separates capture,
  coordination, Glue transfer, publication and pose updates.
- F10 records `player.local.synchronize` and `player.appearance.prepare` alongside
  the existing CPU/source dependency records. Synchronous diagnostic callers
  explicitly select synchronous loading through the same construction path.

- Owned CPU phases now dispatch ready nodes from three intrusive cost bins,
  preserving FIFO ties, prerequisite eligibility and original output slots.
  Node links use charged metadata; there is no whole-scene sort or per-bin
  allocation. Incremental and template admission accept frozen estimates.
- The dispatcher also uses three reserved runner bins within each frame urgency.
  Ready-cost increases move already queued runners; decreases are checked before
  selection. All queue storage is charged at startup and admission counts stay
  unchanged. Only atomic hints cross the phase/dispatcher lock boundary.
  Between kernels, heavier or equal-cost ready phases receive a turn, with FIFO
  ties. Protected-worker exclusion and required/retirement service precedence
  remain independent of cost. Empty runners finish without another service turn.
- Runtime supplies measured hints for M2 root poses, geometry and receiver
  ranges. Main retains calibration; owned jobs carry only work counts and sparse
  timing samples. The first eight jobs are sampled, then one in 64. Failed/partial
  kernels do not update estimates. Geometry separates palette/particle/ribbon
  presence and uses collection lengths, without scanning particles or vertices.
- Receiver ranges target a calibrated 100-microsecond quantum, capped at eight
  ranges per worker. The initial uncalibrated width is 64 receivers. This is a
  scheduling policy; callbacks, light ancestry and final uniform indices retain
  their existing order. Hints never alter clocks, simulation steps or visibility.
- F10 sampled execution spans carry node identity and estimated nanoseconds.
  `cpu.phase.drain_tail` measures last dispatch through last kernel return;
  `cpu.phase.last_return` identifies the last returning node in its phase.
  Publication/main-consumption time is separate. Disabled tracing adds no tail
  clock reads. A selected runner that loses its heavy nodes before claiming work
  yields to a heavier queued phase; F10 records `cpu.frame.cost_preempt`.
  Calibrated bulk/service slicing beyond the connected M2 ranges remains required.

- Live cinematic presentation now services native input while every reader of
  a changing decoded image retires. It uses the renderer's existing dedicated
  completion thread and reusable request/result cell; no general CPU job waits
  for a GPU fence. Ready readers skip dispatch, and unchanged movie identities
  and dimensions do not drain unrelated slots. The exclusive renderer borrow
  pins reader fences and source storage through success, native failure and
  unwind. Movie decode, audio-master time, frame selection and upload order stay
  unchanged. The current F10 name is `rendering.cinematic_source.reader_check`;
  host/native waits are recorded separately as described above.

- CPU now supplies a retained main-thread readiness queue. Its bounded numeric
  nodes and prerequisite subscriptions use the existing storage budget; runtime
  retains all non-Send owners and execution. Registration rolls back on refusal,
  failed fan-in releases unrelated subscriptions, and cancellation withdraws only
  this consumer. Late notifications cannot publish into a reused epoch. A main
  consumer can be admitted even when its producing frame fills worker admission.
- World preparation uses this queue for ground detail, WMO packets, lit surfaces
  and receiver completion. Ground/WMO ordering and receiver callback placement
  stay unchanged. Terrain/liquid consumes published light sources while receiver
  computation can continue; terminal M2 reclamation still returns every worker
  pin. The driver drains permitted notices before its native/offline wait.
  Dropping a partial world phase cancels subscriptions and closes its producers.
- F10 links `cpu.main.request`, `cpu.main.ready` and `cpu.main.consume` to the
  producing phase, alongside `world.main_continuation` and
  `frame_pipeline.main_ready_pending`. This establishes a common continuation
  mechanism and its world consumer; UI/FrameXML order is unchanged. Loading,
  upload/acquire/growth and other main-only consumers still need integration.

- Live Glue creation and roster-selection bodies now use the same namespace-wide
  primary-M2 request adapter as NPC and remote-player appearances. Pending source
  readiness gates an owned appearance phase without occupying a waiting worker;
  ready leases enter the existing construction path directly. Admission refusal
  retains the private archive/cache bank, and ordinary completion restores it
  before publication or error handling. This does not convert nested attachments
  or selection pets; the live local-player path is connected as described above.
- Main owns one selected Glue attempt. Residency or texture-quality changes
  permanently withdraw that attempt; after it drains, main admits the latest
  selection. No worker rereads a shared selection mutex or loops through replacement
  appearances. Facing-only changes reuse residency and apply the latest transform
  at publication. Current failures retain their selected identity without silent
  retry. Withdrawal of a published selection removes its resident model on that
  update, fixing the previous short-circuited removal.
- Glue residency, source selection, admission and pending ownership now have
  focused children under `player_coordinator/glue_character`. The common owned
  dependency adapter lives under `worker_presentation/model_request`. Synchronous
  creation/selection consumers explicitly retain their local-cache mode; both
  modes run the same appearance/attachment/animation construction sequence.

- Receiver-uniform evaluation now fans out over independent contiguous ranges,
  sharing one immutable receiver/ancestry bank and the existing light-source bank.
  Main retains callback order and final scene indices; a late vehicle parent can
  be read from any range because callbacks finish before dispatch. Workers write
  only into output capacity reserved by main. Small scenes retain the single-job
  vector transfer; larger scenes copy completed ranges into the reserved final
  array in receiver order, preserving only the prefix through the first error.
  Reclamation releases all reader pins on success, domain error, worker panic,
  dependency failure and admission refusal. Unused job outputs are dropped when
  the active range count shrinks. Receiver storage, batch lifecycle and evaluation
  have separate children under the scene-lighting folder.
- The initial partition uses 64 receivers per range until measured calibration
  supplies the width, bounded to eight ranges per configured worker. F10 records
  batch/receiver counts and range identities.
  `m2.scene_lighting.evaluate` now measures individual batches; its per-call mean
  must not be compared with the former whole-receiver-phase mean as an FPS gain.
  Nested receiver/output byte accounting and matched
  scaling/live measurements remain required, alongside the barriers below.

- Published M2 point and directional lights now form one retained immutable
  source bank, pinned by receiver work and also readable by main. Receiver-specific
  exterior light is appended through ordered iteration, rather than mutating a
  shared directional vector. Reclamation releases the worker pin on success,
  admission failure, dependency failure and receiver-query failure; next-frame
  mutation requires sole ownership and does not copy an in-flight generation.
- World presentation runs terrain and liquid packet preparation once those
  sources are published, while receiver-uniform evaluation may still execute.
  It retains surface failures until the original M2 consumption boundary, then
  preserves terrain/liquid/WMO error order and the later fog/sky publication.
  Source preparation, surface consumers and final M2 outputs now have separate
  interfaces; source inputs no longer wait inside the final M2 draw bundle.
  F10 links the `m2.light_sources` product to both the receiver worker and
  `world.lit_surfaces.consume`. Surface preparation has its own folder child.
  This removes the source-bank ownership barrier for these consumers, not every
  remaining main-only continuation or receiver-evaluation barrier.

- M2 preparation now retains an explicit continuation through admission, ordered
  geometry publication, geometry/pose reclamation, receiver callbacks and scene
  lighting. Each resume consumes only ready work; an unfinished dependency
  returns the unrelated main owners before the driver selects its exact wait.
  The geometry cursor and capacity totals survive yields, and terminal readiness
  cannot repeat receivers, transparency ordering, effects or random consumption.
- World presentation revisits root-pose admission between its ordered ground-detail
  and WMO packet steps. A pose that becomes ready during ground-detail work can
  now release geometry before WMO preparation finishes. Receiver callbacks and
  fog-bank publication remain after those independent steps. Renderer options
  are captured at admission, so the continuation need not retain its renderer
  borrow. The world driver lives in the `terrain_frame/presentation` folder.
- CPU batches expose non-consuming result/terminal waits for offline drivers.
  An open producer is rejected at terminal wait, avoiding a wait on the caller's
  own future append. Unfinished waits reject CPU workers. Dropping an M2
  continuation restores lighting storage as well as poses and effect state.
  This establishes resumable frame phases and two interleaved main operations;
  the numeric main-ready queue described above now carries world consumers,
  while the complete cross-domain loading/upload continuation graph remains open.

- Live world/Glue, UI/loading and cinematic presentation now service native input
  while their exact next GPU frame slot is unavailable. One rendering-owned
  completion thread performs the host wait; CPU workers never wait for a GPU fence.
  A single reusable request/result cell publishes terminal state before waking
  the coordinator. Ready/unallocated slots skip thread dispatch; the ready path
  performs a status query before the existing presentation wait/reset.
- The scoped renderer borrow excludes fence reset, replacement, submission and
  destruction until the host observer completes. Native error/unwind drains the
  observer; renderer teardown joins its thread before destroying Vulkan children.
  No gameplay callback runs in this wait. F10 records the moved wait in
  `rendering.gpu_slot.pending`; the inner world's fence timing now records
  only its remaining synchronous wait. Frame-slot readiness does not mean asset
  upload readiness. Cinematic shared-source reader waits are connected as above;
  image acquisition, remaining uploads, buffer growth/device-idle waits and
  further main-ready continuations remain required.

- Live world, Glue and world-replay frame consumption now uses an explicit
  main-owned native wait context. Unfinished root palettes yield before placement
  mutation; ordered geometry consumes each ready result directly, then services
  native input while waiting for its exact next result. Final pose, geometry and
  scene-light reclamation observes terminal publication and admission release.
  SDL events stay queued until the ordinary gameplay cutoff. No scene clocks,
  callbacks or simulation steps run from a wait, and no general worker pumps SDL.
- Native-wait failure still drains owned CPU state before returning. Offline
  fixtures explicitly retain synchronous executor consumption; this is a selected
  execution context, not a live fallback on native errors. SDL window ownership,
  input translation and native waiting now have separate folder-module children.
  F10 has distinct result/reclamation native wait spans. Full main-ready service
  and loading dependency integration remain required. GPU-slot native servicing
  is connected as described above.
- Local-player, NPC and remote-player primary M2 loading now joins the same namespace/path
  authority as Glue and top-level GameObjects. A new source has one admitted
  producer; a pending source gates appearance work through `LoadBatch` without
  occupying a worker. Ready leases enter the existing texture, attachment, mount
  and GPU-warmup path directly. Nested models still use the owned local caches.
- Population source waits retain the exclusive archive/cache bank. Admission
  refusal preserves that bank; source failure returns it before reporting the
  original error. Removed/replaced appearances withdraw their own demand and
  cancel unstarted dependent work without cancelling another consumer's decode.
  Withdrawal remains terminal if an appearance switches back before completion;
  the old cancellation is reclaimed before the returning appearance is admitted.
  Producer mount failure publishes the actual archive error to joined owners.
  Population admission, readiness and ordered publication now have separate
  children under a folder module.
- World presentation now uses a scoped M2 admission/publication boundary. Scene
  callbacks launch owned root poses; ordered traversal returns to main before its
  first unfinished root palette, or seals geometry when all selected owners are
  ready. The retained cursor resumes after independent main work without repeating
  model clocks, callbacks, RNG, attachment publication or effect-tail admission.
  Ground-detail selection/publication and WMO packet creation
  run against the admitted scene while those workers can still execute. Final
  geometry consumption, model receiver callbacks, terrain/liquid lighting and
  WMO fog-bank publication keep their ordered boundaries and error precedence.
- The scoped M2 owner restores particle/ribbon state and pose jobs if independent
  preparation unwinds or the caller drops it. This exceptional main-thread
  consumption boundary is profiled separately. Normal completion transfers no
  additional model state and retains the existing per-model ordered publication.
  Scene setup, admission, completion and entry handling have separate children
  under the frame folder; the standalone Glue path drives the same stages.
- F10 separates M2 admission/publication from independent world preparation so
  overlapping work is not counted as M2 execution. Setup, resumed traversal and
  worker descendants share one logical `m2.frame` identity; the readiness yield
  has an explicit observation. Independent world preparation now precedes the
  first root-pose wait. Later unfinished palettes and final consumption use the
  native readiness context described above; full main-ready continuation and
  loading/upload wait integration remain required below.

- Background loading now uses the existing bounded dependency engine through
  `LoadBatch`. Waiting phases retain owned inputs and task admission but occupy
  no worker; each ready kernel runs only on the flexible lane and yields between
  inputs. Frame prerequisite promotion changes service priority without changing
  execution eligibility. Shutdown cancels unresolved loading gates before drain.
- Shared M2 requests expose a per-consumer, one-subscription readiness owner.
  Publication releases source locks before signaling CPU metadata; failure and
  abandonment preserve the source error and suppress dependent kernels. Existing
  selected GameObject consumers retain required demand on the source producer.
- GameObject shared-source joins now start a gated loading phase. M2 publication
  directly releases useful preparation without another coordinator poll. Admission
  failure returns the mounted bank; terminal dependency failure reclaims it before
  the normal publication policy runs. Final instance/transport admission still
  belongs to the main owner. World withdrawal or loss of the last object consumer
  cancels unstarted dependent work and preserves its bank. Combined demand keeps
  other source consumers unaffected; unchanged frames add no membership scan.
  The coordinator now has a folder facade and separate
  pending-task module. This is the first resource-readiness consumer; terrain,
  nested WMO and other loading consumers still need the same integration.

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
- A single-worker pool alternates required background service and frame boundaries.
  With protected workers available, the flexible worker serves required loading
  and retirement until that backlog clears, alternating their finite turns.
  Speculative work runs only when no service or frame work is ready.
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

- Cold task admission now distinguishes required loading, retirement and
  speculation. Speculative reservations leave the final task slot available
  when capacity exceeds one. Queue storage is reserved at startup and demand
  changes move the same queued operation; no resource is loaded twice to promote it.
- Terrain character-location hints and Glue backdrop/texture prewarm are
  speculative; authoritative terrain and selected Glue models promote pending
  work. Detached CPU frees use retirement service. Existing required consumers
  retain FIFO ties and protected-worker exclusion. A running archive/codec/free
  remains indivisible; these are service turns, not measured time slices.

- Immutable archive catalogs now issue a namespace shared by their cloned mount
  plans. Rediscovery creates a distinct generation even at identical paths;
  individual mounted handles retain separate identities. M2/WMO/BLP and encoded
  audio caches qualify source paths by namespace, and decoded samples carry it
  through worker preparation, deduplication and retirement. Merged UI texture
  caches enumerate only the application's namespace for upload.
- Canonical asset paths share immutable string storage across cloned request
  keys instead of copying bytes on cache lookup. Font coverage/metrics use the
  namespace so switching handles within the same immutable plan reuses them;
  a different plan still invalidates the provider's faces. These changes do not
  establish a shared pending-request authority or merge independent model caches.

- General background operations can now keep one owned continuation and yield
  between explicit finite steps. Task identity, admission, captures and the result
  channel survive every yield. Priority changes during execution apply at the next
  enqueue under the queue lock; no per-step task allocation or re-admission occurs.
- CPU retirement transfers its backlog once and releases at most 16 independent
  owners per service turn, in forward order, including during shutdown. Other
  required service can run between turns. The active flexible worker resumes
  without waking protected sleepers for every chunk. One object's destructor,
  allocator free or third-party call remains indivisible; the count bound is not
  a calibrated wall-time guarantee.

- Archive mounting now has an owned continuation that opens one descriptor per
  advance in exact precedence order. Partial stacks cannot be used for reads;
  the synchronous API consumes the same implementation. World UI sources,
  unit-effect sources and configured Glue texture prewarming use the resumable
  worker path, yielding between opens and before domain work. Glue prewarming
  additionally reads/decodes one requested texture per turn and keeps namespace
  alias reuse and ordered per-path diagnostics. Capacity refusal retains the
  original request before any worker captures move.
- The mounted-store implementation and Glue texture source/publication lifecycle
  now have focused folder modules. Single MPQ opens, texture decodes, the full
  FrameXML validation pass and unit-effect preparation remain indivisible calls;
  this establishes explicit domain boundaries without claiming a time bound.

- Added a single-owner typed pending-request index with independent consumer
  generations, strongest direct live demand and one CPU producer per key. CPU
  capacity is reserved before the producer factory can transfer domain inputs.
  F10 request provenance links joins, producer dispatch, consumption and release
  across frames using the existing disabled-path clock/TLS suppression.
  Cancelled producers remain owned until completion/explicit drain; completed
  failures fan out only to current consumers and leave no new negative-cache entry.
- The runtime sound loader now shares exact namespace/path reads across its
  queued voice requests, including nonadjacent A/B/A demand. Decode and voice
  completion retain FIFO order and the existing sample/stream, gain and RNG rules.
  Read pins survive through decoder admission; one cancelled voice cannot cancel
  another consumer. Retired encoded sources use bounded CPU cleanup during frames.
- Sound requests expose a monotonic reservation-lifetime signal. A unique engine
  owner invalidates all observer clones on completion, cancellation or disposal;
  loader pruning checks those signals instead of nesting a pending-voice scan
  for each request. The engine still validates the exact handle before playback.
  Request lifecycle, loading commands and runtime archive/voice scheduling now
  have focused folder modules.

- M2 and WMO decoded-source consumers now hold typed immutable resource leases
  throughout runtime residency, animation/pose preparation, rendering sources and
  systems collision. Cache payload ownership is separate from consumer pins;
  callers cannot extract an untracked owning Arc. Existing live lease clones
  share one pin, and release/reacquire preserves the payload's pointer identity.
- Their source caches collect registered final-release notifications rather than
  scanning every cached model's Arc count. Each entry reserves one reusable
  intrusive notification slot; churn coalesces, stale generations are rejected,
  and payload destruction happens outside the release metadata lock. Existing
  WMO collection boundaries remain. Qualified M2 sources now use stock's signed
  10,000 ms release age. Reacquisition renews release identity, preventing a late
  old-pin destructor from starting the next consumer generation's grace period.
- Catalog clones and their mounted stores share a typed M2 maintenance service.
  Runtime schedules finite retirement steps on the existing CPU pool when a
  release deadline or cache-owner closure requires it, including idle/loading
  frames and offline replay. Cache indices detach entries under metadata locks;
  workers dispose of payloads after unlocking. Pool saturation preserves pending
  ownership, and shutdown observes accepted task results. Ordinary frames check
  a dirty bit, task completion and cached deadline, without scanning models.
  BLP/other cache adoption, request sharing and resource byte admission remain
  required; this retention rule is not a global cache or GPU eviction policy.

- Added namespace-wide pending primary-M2 requests with one producer, independently
  registered consumers and one shared source/error result. Successful publication
  enters the qualified source cache; failed/abandoned producers publish a terminal
  outcome without a persistent negative-cache TTL. Decoder and archive work run
  outside request/source locks. An unfinished request rejects CPU-worker waits.
- Connected Glue model loading and asynchronous top-level M2 GameObjects to that
  request authority. Existing pending requests can be joined before CPU admission,
  including a full queue. Dependent texture/draw preparation receives either useful
  producer work or a published model, never a worker-side waiter. World withdrawal
  drops the matching consumer; publication still uses current scene owners.
  Backdrop archive state now moves into and back out of its job, without a mutex
  around archive/cache work. Request, producer, backdrop dispatch and publication
  responsibilities have focused folder modules.
- CPU service demand now supports independently registered consumers and a shared
  scheduling-control handle that does not share task-result consumption. Required
  joins promote the original producer immediately; withdrawal restores remaining
  speculative/retirement demand. Clones share one consumer registration. Demand
  metadata serializes queue reclassification, invokes no domain callbacks, and
  holds only weak dispatcher ownership. Asset request policy uses this CPU facade;
  the CPU crate still has no asset, rendering or gameplay dependency.

- Glue character and population workers now transfer their complete archive/cache
  bank after CPU admission and return it with ordinary success or failure. Cache
  ownership is restored before stale-result rejection or GPU warmup. The worker
  wrapper receives frozen selected inputs instead of a mutex-protected
  archive/cache bank. Model caches retain their existing metadata synchronization.
- Glue replacement attempts return to main after reclaiming the preceding owned
  task, as described above. One complete appearance construction remains
  indivisible; nested model dependencies and finer bulk stages remain required.
- Terrain prewarm, authoritative entry and neighbor streaming now use the same
  owned CPU continuation. Archive mounting yields between MPQs, followed by
  separate WDT/WDL, ADT, mesh, surface/query and placement stages. MTEX resolution
  yields after each texture and MDDF preparation after each referenced placement.
  The synchronous tile path drives the same implementation. Source order,
  duplicate validation, cache return on domain failure and whole-generation
  publication remain intact. Worker scheduling and tile preparation have focused
  children under the folder-backed terrain coordinator.
  Individual decoding operations, ground-detail preparation, liquid assets,
  nested WMO preparation and terminal cache cleanup remain indivisible. This is
  not a bounded maximum service duration or shared terrain source-request graph.
- Epoch registration now prunes inactive entries using a separate atomic identity
  cell. It no longer locks other batch states or temporarily retains their domain
  owners under the registry lock. Admission publishes the live generation;
  terminal completion clears it before releasing admission. Shutdown still visits
  owners outside the registry lock, and the identity cell is reused across epochs.

## Discovered dependency suspension and ordered terrain sources

The follow-up to Build 173 adds owned service suspension for dependencies
found during loading. Waiting services retain their existing admission and
captures while releasing the worker and bulk allowance. Source completion,
consumer cancellation and shutdown resume the same operation without polling.
Shared-source urgency follows live consumer demand while it is suspended.
The executor closes external suspension before draining logical admission.

Runtime MDDF loading and nested MODD loading in both ADT and global-WMO scenes
now join the shared M2 request authority. WMO registration yields between
MODF owners and selected MODD resources while preserving stock traversal and
whole-generation publication. Tile preparation and WMO residency now have
folder-backed facades and focused preparation/cache/source/material modules.
The dependency state stays in the service allocation, preserving the compact
frame queue record. Disabled dependency timing takes no clock reading.

The focused scheduler and archive-backed terrain tests pass. Final workspace
checks and packaging are recorded in the
[discovered dependency report](cpu-discovered-dependencies.md). This closes a
loading suspension gap. The subsequent changes below connect world recording
and WMO/ground-detail source authority. Main-thread M2 admission, other
unconverted consumers and complete cache/working-set accounting remain required. No frame-rate gain is claimed.

## Parallel world recording and shared scene sources

World command recording now runs as owned contiguous command ranges on the CPU
executor, with independent secondary command pools and frame-slot fence lifetime.
Main records postprocessing/UI while those ranges run, then submits the original
ordered world and compositor streams. Numeric command capture reserves its full
bound before dispatch. This closes the previously serial world-recording/UI
boundary; it does not move ordered gameplay callbacks or queue submission.
The renderer source passed 1,668 workspace tests and the 896-frame, 300-NPC replay;
reviewed before/after captures preserve geometry and compositor output. Details
and measurement limitations are in [world recording](cpu-world-recording.md).

WMO root/group requests now share namespace authority across terrain and
GameObjects. GameObject default-set doodads and terrain ground-detail models
join shared M2 dependencies. Those continuations retain source order and publish
only complete consumers. Source completion, failure and withdrawal are tested on
one worker. WMO release notifications join bounded worker maintenance; a change
bit is checked against actual pending releases so empty caches cannot schedule
cleanup every frame. Ground-detail and GameObject worker code now have folder
facades. See [discovered dependencies](cpu-discovered-dependencies.md).

The first full shared-WMO suite exposed two empty-cache maintenance assertions;
the unnecessary scheduling was corrected. The complete shared-WMO/ground-detail
source then passed 1,674 tests, with zero failures and 33 ignored, plus workspace
Clippy. Further changes yield WMO producers between root/group decode stages and
use deferred staging retirement for generated stock textures. The final combined source passes 1,676 workspace tests (zero failures, 33 ignored),
formatting and workspace Clippy. The optimized candidate completes 1,792 crowded
frames across 1/2/4/8 workers and an 896-frame F10 capture. The paired earlier
binary also completes, but the candidate is slower overall in that pair; no
frame-time gain is claimed. See the [qualification report](cpu-world-recording.md#combined-source-qualification).
[Build 174](testing-build174-continuations.md) packages and installs this source;
its executable identity and hash are verified.

## Still required for the complete cutover

On 2026-09-20 the user directed that the next turn complete this CPU cutover and
accepted dealing with fallout afterward. Prioritize implementing the remaining
architecture and connecting its consumers in that turn. Do not substitute another
isolated performance checkpoint for completion. Preserve stock gameplay ordering
and the restored camera behavior; broader performance tuning follows cutover.
The complete requirements below remain in scope.

The next priority is the remaining main-thread admission and renderer boundary.
Complete M2 assembly/ordering now runs as an owned worker operation; its join and
lighter-scene overhead still need improvement. The user expects work to
be distributed across cores for ordinary mixed populations of 200–300 entities.
M2 geometry already receives `JobContext`; completing every unrelated service
adapter is not a prerequisite for that distribution work. Preserve stock
ordering and compare main-thread time and worker utilization, not just whether
an operation has a context parameter. The requirements below remain in scope.

- Preserve the restored camera/input behavior. After Build 161 the user confirmed
  that the client no longer crashed in their test and camera motion was smooth
  again. The reported camera regression is closed on that live confirmation;
  remaining frame-time spikes and cutover scaling still require measurement.
- JobContext now reaches every production frame/loading and cold/resumable service
  submission. Generic admitted worker lanes serve M2 particle sorting; other typed
  scratch remains with domain jobs. The first optimized overhead comparison is
  recorded in the worker-scratch report; broader scratch adoption and qualification
  under other workloads remain required. Do not repeat the completed adapter work.
- Measure the explicit execution plan under varied worker counts and concurrent
  loading. Finite/bulk eligibility, multiple loading runners, scalable runtime
  defaults and capability/process-ownership reporting are connected. Whole-process
  policy qualification remains; fixed affinity and NUMA placement remain deferred.
- Apply the connected batch publication and sampled lock/queue/wake observations
  to matched live movement and concurrent loading. Synthetic 1/3/5/7-worker
  comparisons are recorded in the [dispatch report](cpu-dispatch-publication.md);
  they do not establish contention as the dominant live cost or complete causal
  product attribution. Keep the central scheduler unless measurements justify
  a replacement.
- Extend typed shared-result leases beyond connected M2 loading edges to the
  remaining domains and main-only continuations.
  Numeric main-ready storage and the four world operations are connected.
  Templates, heterogeneous phase fan-in and frame urgency propagation now exist; resource
  cache/I/O integration still requires its complete concrete dependency graphs.
- Calibrated step-size policy beyond the connected M2 kernels, receiver ranges
  and shadow command-recording cost hints.
  Cost bins within/across typed phases, sparse calibration and sampled drain-tail
  reporting are connected as described above;
  resumable asset/bulk stages beyond retirement, the connected archive mounts,
  terrain stages and Glue texture steps, and
  remaining domain demand transitions beyond terrain/Glue prewarm. External producers
  expose urgency, but those services must still consume it. Frame urgency is
  monotonic within an epoch. Shared M2 primary-request consumers now support live
  priority withdrawal; remaining source domains still need that connection.
- Connect reservations to allocations nested inside domain job state and the
  complete required phase working set. Executor-wide scheduler metadata and typed
  result-page accounting now exist; model output, override buffers and retained
  geometry job records, final frame streams and owned full skeletal palettes now
  adopt it. Dispatched live effect simulation and named CPU bone samples now carry
  reservations as well. Other resident effect owners, ordinary asset buffers
  and caches still require adoption,
  connected working-set admission, explicit trimming and maintenance policy.
- Extend native servicing to loading dependencies, remaining GPU upload waits
  and further useful main-ready continuations. World preparation,
  presentation-slot waits and required shadow-recording joins are connected;
  world/Glue, UI/loading and cinematic acquisition, world resource/quality
  retirement, screenshot readback and native swapchain recreation now use the
  completion service.
  M2 normal consumption now services native input at its necessary waits;
  exceptional abandonment retains unconditional CPU state reclamation.
- Extend cross-domain overlap beyond the connected ground-detail/WMO and
  terrain/liquid consumers, including phase-specific M2 demand. World command recording now overlaps
  main-owned UI/compositor recording.
  Ordered receiver callbacks, receiver-uniform completion for M2 draws and
  end-of-frame state reclamation still have barriers.
- Extend pending-request authority beyond runtime audio, Glue backdrops and
  creation/selection primary M2s, asynchronous top-level GameObject M2s and
  local/NPC/remote-player primary M2s. Terrain MDDF/MODD/ground detail and GameObject WMO sources are now connected.
  Glue attachments/pets and population attachments/mounts/item visuals now join
  the same namespace source authority through resumable appearance tasks.
  Sky models and stars now join the same authority through resumable source tasks.
  Effects still require their remaining shared-source connection.
  Other source domains still need shared pending authority, cross-resource I/O
  dependencies and the remaining domain-wide shared result leases. WMO requests
  now share one root/group producer, yielding between independently resolved groups. M2/WMO sources
  already have external leases and coalesced final-release delivery.
  Ready request pins are not the complete retained-cache/external-lease lifecycle.
  Request metadata and encoded payload budgets still need admission/accounting.
  Stock-evidenced animation demand and
  retention, derived cache invalidation, byte-budgeted residency and GPU retirement
  from the resource design.
- Full causal wait/queue attribution, overhead/scaling checks, and matched
  movement/loading/live measurements. No FPS gain is established by compilation
  or synthetic correctness tests.

These are remaining implementation requirements, not optional deferred scope.
Build 161 was packaged and installed on 2026-09-20 from `32400768`. Geometry
output buffers now follow source generation and visible/shadow demand instead
of accumulating unrelated models' capacities by traversal ordinal. This repairs
a retention defect found after Build 160 exhausted the 128 MiB CPU Frame budget;
it preserves that budget and the camera sampling correction. Formatting, Clippy
and all 1,603 workspace tests pass (33 existing ignored), and the hidden
896-frame movement replay completed. The exact live scene and remaining camera
hitches are not reproduced by that fixture; see [the package record](testing-build161-storage.md).

Build 160 was packaged and installed on 2026-09-20 from `dcd1e4e4`. It restores
clock-dependent camera recovery sampling before reusing the shared
terrain/presentation collision result. Unchanged sampled poses retain spatial
reuse. Formatting, Clippy and all 1,600 workspace tests pass (33 existing ignored).
The visible-motion regression remains open pending a matched movement comparison;
see [the package record](testing-build160-camera.md).

Build 159 was packaged and installed on 2026-09-19 from `8a4ae681`. It adds
retained static topology publication when static indices remain unchanged,
retaining owned static M2 admission, corrected CPU/native wait attribution, owned render
palettes/shadow packets and bounded small-model dispatch groups,
retaining local-player worker construction and shared primary requests, cost
arbitration across phases, measured M2 job ordering and receiver partitioning,
retaining cinematic
shared-source reader waits, the main
readiness queue/world integration, shared Glue appearance sources, receiver-uniform
batches, the shared light-source bank, terrain/liquid overlap, resumable M2 phases,
native GPU presentation-slot and CPU waits, and preceding source/loading changes.
Full source validation, optimized package compilation and
installed executable identity/hash verification passed, as recorded below.
The package does not complete the cutover or establish a measured FPS gain.

## Archive table investigation

A read-only census of direct MPQ headers under the installed Data root and enUS
found 20 candidate archives with 9,898,160 bytes of classic hash/block table
payload per complete candidate set. This counts header entries times the pinned
backend's 16-byte entry sizes; it is not a runtime owner census, allocation-capacity
measurement or RSS result. The backend owns tables per Archive, and its optional
parallel reader reopens an Archive per read rather than sharing those tables.
Duplicate stacks therefore remain a reuse opportunity, but this payload alone
does not establish the cause of the reported roughly 1.2 GB memory difference.
The local census is recorded in ignored `target/archive-table-census.json`.
No backend replacement or hidden parallel pool was introduced.

## M2 retention qualification follow-up

The fingerprinted stock exports identify `+0x144` as the resource's hash-table
backlink. `0081C698` through `0081C6CA` inserts a newly constructed resource unless
lookup flag `0x8` is set; it also repairs the successor's backlink. `0083D5B0`
unlinks those exact fields. `0083DC90` requires both a cache owner at `+4` and
that backlink before timestamping a final release. A lookup hit occurs before
the insertion gate, so flag `0x8` alone does not make an existing cached hit
unqualified. The late-readiness/animation and all caller-flag paths remain open.

The expanded `tools/ghidra/model_cache_lifetime_oracle.py` ran 112 cases against
the pinned executable on 2026-09-15. It executes the original insertion block
with empty/nonempty buckets, flags `0`, `8`, `0x40`, `0x48`, then original release,
reacquisition and collection. It also covers absent cache ownership and confirms
that collection uses signed 32-bit elapsed subtraction, including the sign-bit
boundary and forced collection. Clock, destruction and allocator free remain
controlled boundaries; this does not emulate complete construction or gameplay.
Results are in ignored `target/model-cache-qualification.json`. This evidence
now underpins qualified M2 retention: final consumer release starts a 10,000 ms
cache-clock grace period; reacquisition withdraws it and renews the release ticket.
Standalone leases and WMO caches keep their own existing lifetime policy. This
does not authorize guessed lookup flags, forced pressure eviction, or animation
readiness behavior.

## Checkpoint validation

### Build 159 live review

The 2026-09-20 F10 run confirms dynamic-only topology publication is active:
290 partial and 89 complete publications in the selected world window. Partial
metadata publication averages 0.143 ms. Ordinary world frames still average
8.454 ms, with a 32.304 ms maximum; main consumes 97.46% of one logical CPU while
workers use about 9-12% each. M2 CPU preparation remains 3.665 ms per world frame;
the large apparent drop in the first admission phase includes work redistributed
into later main continuations.

The largest world spike contains 9.499 ms creature/player residency followed by
8.917 ms unit-state updates. Both paths still have repeated searches across the
full placement bank: per-input retained-generation searches, then dirty-topology
owner lookups before render metadata publishes. Correct current owner identity
independent of deferred render metadata remains required. Loading also contains a
131 ms UI slice and 68 ms scene GPU publication. See the
[complete capture review and attribution limits](testing-build159-performance.md).
Only documentation changed in this review; no additional build or fix is claimed.

### Build 159 package checkpoint

The numbered Testing artifact was built from
`8a4ae6813ff09df27202540a1737f4541c3c32cc` using
`scripts/build-client.ps1 -Profile test-client` and installed with `-SkipBuild`.
Tracked source and index stayed frozen during compilation; `dirty=true` records
the reserved `BUILD_NUMBER` change. Optimized compilation completed in 4m54s and
the package helper exited zero. The parent PowerShell exit of one was native
stderr wrapping, not a compiler failure; helper completion, artifact identity
and installed hash were checked together.

Installation completed at `2026-09-19T17:35:57.2007710-04:00`. The installed and
compiled executable SHA-256 values both equal
`3A3E7C1E4E9521EE2B685EAA3A2B001502FB973F557F356001AA06B4AD5D20B1`.
The executable identity, installed manifest and desktop shortcut were verified.
Testing retains four CPU workers/capacity 256, two network workers, 2560 x 1440
fullscreen-windowed, GPU 0 and opt-in F10 capture. No interactive client was
launched. Artifacts are `target/m2-retained-publication-{package,install}.*`.

The latest available live F10 capture remains Build 155, `1789843563290-1`,
ending at 14:47 on 2026-09-19. The later 16:18 client log is Build 157 without
an F10 capture. Consequently neither the current publication measurement nor
the hidden replay establishes a live FPS improvement for Builds 156-159.
This package does not complete the remaining cutover requirements.

### Retained static topology publication checkpoint

Workspace formatting and Clippy across all targets/features with warnings denied
passed. The full workspace test run passed **1,597 tests**, with 33 ignored and
zero failures (94 result summaries). Artifacts are
`target/m2-retained-publication-final-{fmt,clippy,tests}.*`; helper exit was zero.
The initial Clippy attempt rejected a duplicate test-fixture module declaration;
the shared fixture now has one declaration, and the complete final checks passed.

Four new external tests cover the structural journal, effect suffix ordering,
retained static required-work entries and partial replicated-WMO membership.
The existing 26,000-scenery fixture compares all published metadata and work
queries with the full scalar oracle through dynamic removal/append, actual static
relocation, source compaction, reused owners and replacement of the entire scene.
See [the publication contract](m2-topology-publication.md) for the distinction
between retained static indices and structural changes requiring full publication.

The optimized manual fixture alternated 40 complete cached publications and 40
dynamic publications per run, retaining 26,000 static and 28 dynamic placements.
Both paths include the same outer topology/membership operation. Three runs of
the same optimized executable produced the following mean milliseconds per
publication (the raw fields ending in `total_ms` measure the complete operation):

| Run | Complete cached publication | Dynamic publication |
| --- | ---: | ---: |
| 1 | 1.033555 | 0.005457 |
| 2 | 1.066272 | 0.005655 |
| 3 | 1.082935 | 0.006630 |

This removes approximately 1.03-1.08 ms from this fixture's publication operation.
It does not measure total live frame time, insertion/removal costs, actual static
relocation or an older executable. The first optimized compilation took 9m14s.
A documentation change prompted a redundant second Cargo compile; that owned
process tree was stopped, and rounds two/three used the completed first binary
directly. All three benchmark tests passed. Logs and the corrected repeat runner
are `target/m2-retained-publication-benchmark*`.

The hidden populated-world replay passed 168 frames across seven phases with
Soap and 48 NPCs, real terrain/FrameXML and Vulkan. Primary and all three
environment shadow maps produced packets throughout the replay. Its capture
`1789853334350-1` reports zero dropped samples/events/trace rows and zero capacity
overflows; stdout/stderr contain no warning/error. It exercised the complete
initial publication, but no subsequent dynamic-only publication was recorded;
the external mutation/oracle tests and optimized fixture cover that path.
Artifacts are `target/m2-retained-publication-smoke*`. This is a debug functional
replay with a 32 MiB stack, excluding live networking, movement solver, audio and
overlays. Its timings are not live FPS evidence.

### Build 158 package checkpoint

The numbered Testing artifact was built from
`2646c03dccedca892f2b27438ab8cd1ba2c8018b` using
`scripts/build-client.ps1 -Profile test-client` and installed with `-SkipBuild`.
Source and index remained frozen during compilation; `dirty=true` records the
reserved `BUILD_NUMBER` change. Optimized compilation finished in 4m42s and the
package helper exited zero. Parent PowerShell's native-stderr wrapping was not a
compiler failure; the completed artifact, helper exit and installed identity
were checked together.

Installation completed at `2026-09-19T16:50:45.7764625-04:00`. The installed and
compiled executable SHA-256 values both equal
`8F6538A0A4674261FE99401121B75172EE5F69A7A1B54141ED8AA5FFE361D560`.
Executable identity, installed manifest and desktop launcher target were verified.
The launcher retains four CPU workers/capacity 256, two network workers,
2560 x 1440 fullscreen-windowed, GPU 0 and opt-in F10 capture. No interactive
client was launched. Artifacts are `target/m2-spatial-package.*` and
`target/m2-spatial-install.*`.

The source checks and hidden replay are recorded below. This is a connected
static-admission checkpoint, not completion of the full cutover or a measured
live FPS gain.

### Owned static admission checkpoint

Workspace formatting and Clippy across all targets/features with warnings denied
passed. The full workspace test run passed **1,593 tests**, with 33 ignored and
zero failures (94 result summaries). Artifacts are
`target/m2-spatial-validation-{fmt,clippy,tests}.*`; helper exit was zero.
Only ownership-summary comments changed after those source checks.

The three new external tests exercise independent offscreen/hidden shadow and
light demands, WMO collector masks, the native fade cutoff, interleaved model
order, multiple groups, malformed inputs, abandoned suffixes, budget refusal and
reuse. The frozen serial geometry comparison now includes immutable scenery
whose placement indices relocate after dynamic removals; packets, palettes,
particles, ribbons and shared RNG remain equal through moving frames.

The hidden populated-world replay passed 168 frames across streaming, stationary,
orbit, pointer, travel-out, travel-back and settled phases with Soap and 48 NPCs.
Primary and all three environment shadow maps produced packets. Sampled trace
records show static admission on the three protected workers, with observed group
lengths of 6–64. The capture reported zero dropped samples/events/trace rows and
zero capacity overflows; stderr contained no warning/error. Artifacts are
`target/m2-spatial-smoke*`, with capture `1789850594757-1` in its isolated profile.
This was a debug functional replay with a 32 MiB stack, using installed terrain,
FrameXML and Vulkan. It excludes live networking, movement solver, audio and
overlays; its timings are not live FPS evidence.

This connects static admission to the CPU executor. It does not establish a live
FPS improvement or complete dynamic M2 admission, topology publication or the
remaining full cutover scope.

### Build 157 package checkpoint

Build **157** compiled through `scripts/build-client.ps1` in 6m37s from
`22ea7197d0d86eef654c779d6f0ed55140703838`. Tracked source and index stayed fixed
during compilation; `dirty=true` records only the reserved `BUILD_NUMBER`.
Package and installed executable SHA-256 match
`B0F279A94A1E76AFBBEBE9644D6A1456B3924EC86F5367FC9C164637EEE74F47`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers and GPU zero, with F10 opt-in. No interactive client was launched.

The source checks and observer experiment are recorded below. This package
corrects measurement boundaries; it does not establish a live FPS gain or finish
the complete cutover. Logs remain in ignored `target/cpu-consumption-package.log`
and `target/cpu-consumption-install.log`.

### CPU consumption and native-wait tracing checkpoint

On 2026-09-19 the full workspace suite passed 1,590 tests: zero failed, 33 ignored,
94 summaries. Formatting and full workspace Clippy with warnings denied passed;
the final test-isolation adjustment also passed all 47 CPU/rendering library tests.
The CPU fixture proves pending probes and ready callbacks/reclamation create no
false waits, a real gate wait keeps phase/node cause, and unwind/later-frame
consumption retains ownership and provenance. The controlled GPU fixture proves
request-to-host linkage and ready observation without a drain wait.

The optimized ready-result observer experiment measured medians of 54.08 ns/call
with capture disabled, 187.34 ns ordinary and 331.65 ns sampled. These are complete
API-call costs, not whole-frame overhead or FPS gains. Nine captures had no dropped
records/samples or capacity overflows. [Detailed evidence and scope meanings](cpu-consumption-tracing.md)
record the limits and renamed counters. Production behavior, scheduling and the
remaining complete cutover scope are unchanged by this diagnostic correction.

### Build 156 package checkpoint

Build **156** compiled through `scripts/build-client.ps1` in 5m46s from
`b34a6859605507f7cee36096621b43828d744040`. Tracked source and index stayed fixed
during compilation; `dirty=true` records only the reserved `BUILD_NUMBER`.
Package and installed executable SHA-256 match
`F4D30A63914CD25ADE47434D572B4F92779589BFA162E7FE20BBF87EF6CC7FA6`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers and GPU zero, with F10 opt-in. No interactive client was launched.

The full source checks and hidden functional smoke are recorded below. The smoke
trace places shadow-packet preparation on all four CPU workers and includes
multi-model dispatch groups. This confirms the boundary is connected; it does not
establish a live FPS gain or complete the CPU cutover. Evidence remains in ignored
`target/m2-owned-draw-package.log` and `target/m2-owned-draw-install.log`.

### Owned M2 draw-phase checkpoint

On 2026-09-19, formatting, full workspace Clippy with all targets/features and
warnings denied, and all 1,588 workspace tests passed: zero failures and 33 ignored
across 94 summaries. The moving serial comparison includes offscreen shadow-only
jobs, exact primary/environment shadow packets and palettes, checked relocation,
unchanged effect state, failure and abandonment. Two deterministic tests cover
group boundaries and transactional refusal/budget ownership. Logs are retained in
ignored `target/m2-owned-draw-final-*`.

A hidden debug replay completed 168 frames across streaming, stationary, orbit,
pointer, outward travel, return and settled phases at the recorded Orgrimmar
fixture. It used Soap's appearance, 48 authored NPCs, installed terrain/FrameXML,
Vulkan, both shadow banks, four CPU workers and 2560 x 1440. No warning/error logs,
dropped trace/event rows, dropped samples or metric-capacity overflows occurred.
Only the debug diagnostic executable uses a 32 MiB Windows stack. Its isolated
profile disables sound; it has no network or live movement-solver coverage.
The unoptimized timings are functional evidence, not a desktop FPS comparison.
Logs, CSV and capture remain in ignored `target/m2-owned-draw-smoke*`.

Main spatial admission, ordered callback/receiver work, broad topology publication
and the complete requirements above remain open. In particular, the ready-result
wait-attribution defect identified in the Build 155 audit is not changed here.

### Build 155 live F10 review

The user's 65.8-second moving capture on 2026-09-19 is reviewed in
[Build 155 performance](testing-build155-performance.md). Ordinary frames average
9.059 ms. Main consumes 98.44% of one logical CPU while individual workers consume
10.73-13.10%; M2 admission remains 3.424 ms/frame on main. Movement exposes broad
placement publication and terrain-service spikes; UI render-plan preparation
also has a 13.045 ms texture-quad phase outlier. GPU samples average 3.401 ms.
These are current costs, not a matched before/after regression or gain.

The review identifies incorrect wait attribution: `cpu.frame.result_wait` includes
ready-result consumption and lease return. Detail-frame observer perturbation is
also material. Neither should be interpreted as native blocking. Memory plateaus
and later declines during the capture, so this run alone does not establish a
leak. Remaining serial M2 admission, broad residency/UI publication, instrumentation
correction and the existing complete cutover requirements remain open. This was
an analysis/documentation checkpoint; the installed executable is unchanged.

### Build 155 package checkpoint

Build **155** compiled through `scripts/build-client.ps1` in 4m49s from
`4dd6a28ddda06034aaa8be34b688f0cf06150167`. Tracked source and index stayed fixed
during compilation; `dirty=true` records only the reserved `BUILD_NUMBER`.
Package and installed executable SHA-256 match
`E9977E97B0875D11CEE8B0278929DD80052368D65A3FA19C31D75FD6880F1EA3`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers and GPU zero, with F10 opt-in. No interactive client was launched.

The full source checks and hidden functional smoke are recorded below. This
package does not complete the CPU cutover or establish a live FPS gain.
Evidence is retained in ignored `target/local-appearance-package.log` and
`target/local-appearance-install.log`.

### Local-player worker construction checkpoint

On 2026-09-19, formatting, workspace Clippy with warnings denied and all 1,586
workspace tests passed, with zero failures and 33 ignored across 94 summaries.
Four new local-consumer checks cover saturated admission, a shared pending source
without a waiting worker, exact lease sharing, failure/cache recovery, current
motion on publication, appearance reversion, world replacement with a reused GUID,
and serial/worker atlas, geoset, mount, collision and camera output parity. Existing
camera, equipment, mount and moving M2 scene fixtures remain in the passing suite.
Logs are in ignored `target/local-appearance-verified-*`.

A hidden debug smoke run completed 56 frames across streaming, stationary, orbit,
pointer, outward travel, return and settled phases using Soap's appearance and
12 authored NPCs at the recorded Orgrimmar fixture, with installed terrain,
FrameXML and Vulkan. No warning/error logs were produced. The diagnostic's
default Windows main stack overflowed before scene output; rebuilding only that
example with a 32 MiB stack completed successfully. The smoke exercises the common
construction/render path; its player initialization remains synchronous. The
controlled tests above exercise asynchronous admission. This is functional
evidence, not a matched live FPS measurement or a network/audio/movement-solver
test. Logs/CSV are in ignored `target/local-appearance-smoke*`; the initial stack
failure remains recorded in `target/local-appearance-world*`.

### Build 154 package checkpoint

Build **154** compiled through `scripts/build-client.ps1` in 6m21s from
`f5a9540bcd57cda92733d30457b5a3e424b7bbd0`. Tracked source and index stayed fixed
during compilation; `dirty=true` records only the reserved `BUILD_NUMBER`.
Package and installed executable SHA-256 match
`0C6F09C478BC89992CD81C38A9B9447916FDBAC8CB85A25E16C1EC8E1D25CAEE`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers and GPU zero, with F10 opt-in. No interactive client was launched.

The full source checks passed as recorded below. This package does not complete
the CPU cutover or establish a live FPS gain. Evidence is retained in ignored
`target/phase-cost-package.log` and `target/phase-cost-install.log`.

### Cost arbitration across phases checkpoint

On 2026-09-19, formatting, workspace Clippy with warnings denied and the full
workspace suite passed: 1,582 tests, zero failures and 33 ignored across 94
test summaries. Seven new controlled checks cover urgency before cost, FIFO
ties, an already queued phase gaining expensive work, cancelled high-cost heads,
dependency release, required-service turns, and concurrent admission with epoch
reuse on two and four workers. The warmed graph/fan-in fixture now includes
competing phases and still records zero allocator calls over 1,000 activations.
Moving M2 pose/geometry and stock rendering fixtures remain in the passing suite.

Logs are in ignored `target/phase-cost-final-{fmt,clippy,tests}.{stdout,stderr}.log`.
The added runner buckets reserve and charge metadata at executor creation;
admission bounds do not increase. These checks establish scheduler ordering,
ownership and warmed allocation behavior. They do not measure whole-client
overhead, live frame-time improvement or completion of the remaining cutover.

### Build 153 package checkpoint

Build **153** compiled through `scripts/build-client.ps1` in 5m40s from
`967ca32500148c9b337c3b66ba05988078fc2ba6`. Tracked source and index stayed fixed
during compilation; `dirty=true` records only the reserved `BUILD_NUMBER`.
Package and installed executable SHA-256 match
`BF63A89345C7D91760884B8B554C199D8692BEF14B869A7DD44AA10E3D8468E0`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing uses 2560 x 1440
fullscreen-windowed, four CPU workers, capacity 256, two network workers and
GPU zero, with F10 opt-in. No interactive client was launched.

The complete source checks and M2 parity results are recorded below. This
package does not complete the CPU cutover or establish a live FPS gain.
Evidence is retained in ignored `target/cost-dispatch-package.log` and
`target/cost-dispatch-install.log`.

### Calibrated M2 job dispatch checkpoint

On 2026-09-19, formatting, full workspace Clippy with warnings denied and the
full workspace test suite passed: 1,575 tests, zero failures and 33 ignored across
94 summaries. Seven new CPU tests cover expensive-first dispatch, FIFO ties,
dependency release, cancellation/epoch reuse, urgency precedence, transactional
hint-count rejection, calibration arithmetic and sparse timer selection. The
warmed graph/fan-in fixture still allocates nothing across 1,000 activations
while using all three cost bins.

Moving-camera pose and full moving M2 geometry parity pass. Controlled receiver
calibration changes the actual partition from two to 32 jobs with identical
final uniforms. The F10 provenance test verifies node/estimate and final-return
identity across a frame boundary. Logs are in ignored
`target/cost-dispatch-final-*`, with totals in
`target/cost-dispatch-validation-summary.json`. These establish behavior and
storage contracts; live FPS gains, cost-model accuracy and cross-phase cost
arbitration are not established by this checkpoint.

### Build 152 package checkpoint

Build **152** compiled through `scripts/build-client.ps1` in 5m23s from
`464ee0d1516d0fb86acf32065d92614899d81fe8`. Source and index stayed fixed during
compilation; `dirty=true` records only the reserved `BUILD_NUMBER`. Package and
installed executable SHA-256 match
`CEB7A6A1024D29B094812343B4C18EBA6429501D31E455FA7196B51D1E66745A`.
Installation used `install-test-client.ps1 -SkipBuild`; identity, manifest,
launcher and desktop shortcut were verified. Testing retains 2560 x 1440
fullscreen-windowed, four CPU workers, capacity 256, two network workers and
GPU zero, with F10 opt-in. No interactive client was launched.

The source passed the full validation and hidden Vulkan cinematic fixture below.
This closes cinematic shared-source native waiting, not movie decoding, general
upload/acquisition waits or the full CPU cutover. No live movie latency or
in-world FPS improvement is established. Evidence is retained in ignored
`target/cinematic-readers-package.log` and `target/cinematic-readers-install.log`.

### Cinematic source-reader checkpoint

Formatting, workspace all-target/all-feature Clippy with warnings denied, and
the full workspace suite passed: **1,567 passed, zero failed, 33 ignored** across
93 summaries. Controlled completion tests verify that all unfinished readers
are observed, ready readers skip dispatch, and native failure drains the current
observer without touching subsequent readers. Existing panic, driver failure,
notification and shutdown checks also passed.

The hidden Vulkan cinematic fixture now checks no-reader and unchanged-source
paths, changed frame identity, source dimension replacement and the new image's
actual center pixel. Repeated display refreshes retain their authored source.
Existing cinematic deadline tests moved unchanged into the external test tree.
These checks establish readiness/lifetime and pixel behavior, not a live movie
latency or in-world FPS improvement. Logs are in ignored
`target/cinematic-readers-focused.log`, `target/cinematic-readers-final-*` and
`target/cinematic-readers-validation-summary.json`.

### Build 151 package and main continuation replay

Build **151** compiled through `scripts/build-client.ps1` in 5m41s from
`fffb70e23eeceddf78606c596ee7b5016f617da3`. Source and index remained fixed through
package and replay compilation; `dirty=true` records the reserved `BUILD_NUMBER`.
The package and installed executable share SHA-256
`AADF9C559FCD0EA412433EAE81CEEEA1E75F5F9FB35431D6F0D921CEE19B63AF`.
Installation used `install-test-client.ps1 -SkipBuild`; executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains 2560 x
1440 fullscreen-windowed, four CPU workers, capacity 256, two network workers
and GPU zero. F10 remains opt-in. No interactive client was launched.

The optimized hidden world replay completed **896 frames**, 128 each of
streaming, stationary, orbit, pointer, outward travel, return and settled phases.
It used Soap's saved appearance and 120 authored NPCs, installed terrain/FrameXML/
Vulkan, map 1 at (1515.34, -4417.27, 18.0499), travel offset (-80, 0, 0), 2K,
four CPU workers and an isolated profile with audio/VSync disabled. It admitted
one neighboring tile, with one to two resident tiles and no evictions.
M2 packets ranged from 54 to 1,178 and bone transforms from 401 to 15,305.
Capture `1789834878039-1` contains 28 sampled main requests, each with exactly one
ready notice and one consumption. There were zero dropped samples/events/trace
rows and zero capacity overflows; the replay logs contain no warnings or errors.

This exercises actual world preparation and rendering under camera/content
changes, but not live network, audio or the movement solver. Hidden offline times
do not establish a matched live FPS gain or complete the remaining cutover.
Evidence: ignored `target/main-ready-package.log`, `target/main-ready-world*`,
`target/main-ready-world-profile/Profiles` and `target/main-ready-install.log`.

### Main readiness and world continuation checkpoint

Formatting, workspace all-target/all-feature Clippy with warnings denied, and
the full workspace suite passed: **1,565 passed, zero failed, 33 ignored** across
93 test summaries. New coverage checks worker/main/worker dependencies with
non-Send main state, subscription rollback, registration/publication races,
failed fan-in with a never-ready sibling, cancellation/reuse, admission while
worker capacity is full, and rejection of worker-side waits. A separate fixture
observes zero allocations across 1,000 warmed queue epochs. A capture fixture
verifies main readiness links to the actual producing CPU phase across frames.

World tests cover surfaces before receiver completion, ground failure without
preventing model cleanup, and abandoned-frame isolation. Existing moving M2,
receiver lighting and rendering parity checks passed in the workspace run.
The fixture's allocation result covers scheduler metadata only; it does not
establish whole-client allocation behavior, live FPS or complete domain memory
accounting. Logs are in ignored `target/main-ready-final-*` and
`target/main-ready-validation-summary.json`.

### Build 150 package and Glue transition checkpoint

Build **150** compiled through `scripts/build-client.ps1` in 4m43s. The subsequent
Cargo dependency inventory rebuilt the same frozen revision in 4m40s; it reserved
no additional number. Source and index remained at
`70e035ce67fe87a23f2aead87ced98460b7889b7` during compilation; `dirty=true`
records the reserved `BUILD_NUMBER`. Final package and installed executable
SHA-256 match
`494F80B1021E19095C947BA116737E22A3DC500E5A3BE145838D1152ACEEC2E9`.
Installation used `install-test-client.ps1 -SkipBuild`. Executable identity,
manifest, launcher and desktop shortcut were verified. Testing retains
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers and GPU zero. F10 remains opt-in; no interactive client was launched.

A hidden optimized replay completed **31 Glue transitions and 3,623 frames**:
3,127 transition frames plus 16 following frames per step. It covers cold/warm
Human, Night Elf and Blood Elf selections, an equipped hunter and pet, ghost
transitions, creation race/class changes, both directions on all five
customization controls, returns to prior races and randomization. It ran against
installed assets at 2560 x 1440 with four CPU workers, an isolated profile and
audio disabled. The existing benchmark drives the live Glue presentation path
and requires complete scene readiness at each step. Capture
`1789831738343-1` reported zero dropped samples/events/trace rows and zero capacity
overflows. These hidden, short, offline pre-world intervals are not evidence of
visible in-world FPS improvement, world movement, networking or live audio.

The ignored harness copies `benchmark_glue_transitions` with only its startup
changed to `start_hidden`, using the existing 32 MiB diagnostic stack convention.
Its first compile selected incompatible cached library variants; Cargo's exact
artifact inventory corrected that selection. A second link exposed the missing
thin-LTO setting; using the package's thin-LTO setting completed the harness.
Neither failure changed client source. Evidence is retained in
`target/glue-shared-package.log`, `target/glue-shared-artifacts.jsonl`,
`target/glue-shared-hidden.{csv,stdout.log,stderr.log}`,
`target/glue-shared-hidden-summary.json` and the isolated profile's `Profiles`
directory. Full source validation is recorded below. The cutover remains active.

### Shared Glue appearance source checkpoint

Final formatting, workspace all-target/all-feature Clippy with warnings denied,
and the complete workspace suite passed: **1,552 passed, 0 failed, 33 ignored**
across 90 suite summaries. Six production-coordinator tests replace the former
three worker-step tests. They cover pending/ready primary-source sharing,
sole-worker availability during a source wait, latest-selection publication,
independent withdrawal, abandoned-source failure and cache return, admission
refusal, quality invalidation and immediate resident removal. The selected
appearance also matches synchronous atlas mips, geosets and facing. Existing
population and M2 movement/lighting parity coverage passed in the full suite.

The first focused run used an invalid fixture texture level of zero; the fixture
now uses stock's supported level eight. The final full run includes quiet handling
of expected selection cancellation. Logs are in ignored
`target/glue-shared-final-{fmt,clippy,tests}.{stdout,stderr}.log` and
`target/glue-shared-validation-summary.json`. These establish source ownership
and parity contracts, not the complete cutover or a measured FPS gain.

### Build 149 package and populated world checkpoint

Build **149** compiled through `scripts/build-client.ps1` in 4m42s and was installed
with `install-test-client.ps1 -SkipBuild`. Source and index remained frozen at
`86625e3b53108916d9061d2ea4cec8d5e8134578`; `dirty=true` records the reserved
`BUILD_NUMBER`. Package and installed executable SHA-256 match
`F79DD3D500AE3E70C7A7FB07E168BDBD29B9453382A767AF45A757896D8C464F`.
Executable identity, manifest, launcher and shortcut were verified. Testing uses
2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256, two network
workers, GPU zero and opt-in F10 capture. No interactive client was launched.

The optimized hidden world harness completed **896 frames**, 128 in each of the
seven existing phases, with Soap and 120 authored Goblin NPCs. Display 6882 was
verified from the installed stock `CreatureCatalog` as `GOBLINMALE.MDX`. NPCs used
an explicit local grid; this is not the server population. Starting position,
travel offset, resolution, shadow quality and isolated audio/VSync settings match
the Build 148 harness description below. It exercised 54-1,178 M2 packets and
0-4,904 particle vertices per frame. Residency increased from one tile to two,
with one admission and no evictions. No network session, live movement solver or
audio was exercised, and hidden frame times are not matched live FPS evidence.

Capture `1789828523275-1` reports zero dropped samples, trace/event rows or capacity
overflows. It observes up to **217 receivers** and **four batches** per frame.
Receiver work ran on all four CPU workers; the seven detailed frames contain
20 receiver spans, including three overlapping spans on distinct workers.
This verifies the fan-out is connected through the production world presenter.
The initial partition still needs calibration and complete cutover requirements
remain open. Evidence: ignored `target/receiver-batches-package.*`,
`target/receiver-batches-world.*`, `target/receiver-batches-world-summary.json`
and `target/receiver-batches-world-profile/Profiles/`.

### Independent receiver-batch checkpoint

On 2026-09-19, formatting, full workspace/all-target/all-feature Clippy and the
full workspace/all-feature tests passed: **1,549 passed**, **33 ignored**, zero
failures across 90 suites. Six new tests cover one/four-worker execution with
empty, small and multi-range scenes; changing receiver counts; parents in a later
range; and repeated reuse of the same source and receiver banks. A controlled
first-range hold proves a later range completes on another worker before main
publishes any uniform. Final scene arrays match the single-range reference.

Separate tests verify exact partial output through the first malformed receiver,
failed dependency cleanup, refusal at full admission followed by a successful
frame, and reader-pin return after a caught worker panic. Existing stock lighting,
moving M2, visibility, liquid/fog and renderer fixtures passed unchanged. Logs are
in ignored `target/receiver-batches-final-*`. These results establish parallel
execution and output/lifetime contracts, not calibrated partition sizes, full
cutover completion or a live FPS gain.

### Build 148 package and hidden world checkpoint

Build **148** was reserved and compiled through `scripts/build-client.ps1`, then
installed through `install-test-client.ps1 -SkipBuild`. The optimized build
completed in 5m16s. Source was frozen during compilation at
`a9552fd5a6f44b68c35ab6cf81240072c8278b66`; `dirty=true` records only the reserved
`BUILD_NUMBER`. Installed executable and package SHA-256 both match
`D7DEA758FBD56E08CC90088D2B00442B17AC2971007BF13C505A4E4192E21963`.
The manifest, executable identity, launcher and Desktop shortcut were verified.
Settings remain 2560 x 1440 fullscreen-windowed, four CPU workers, capacity 256,
two network workers and GPU zero; F10 remains opt-in.

The optimized `benchmark_world` harness also completed **448 hidden frames**, 64
each of streaming, stationary, orbit, pointer, outbound, return and settled.
It used installed assets and Soap's saved appearance on map 1 at
(1515.34, -4417.27, 18.0499), travel offset (-80, 0, 0), 2560 x 1440, four CPU
workers, shadow quality 5 and an isolated profile with audio and VSync disabled.
The route exercised 0-32 terrain packets, 2-313 WMO packets, 0-217 M2 packets and
0-2,364 particle vertices per frame. One resident tile remained throughout, with
no tile admissions or evictions. There were no authored NPCs, network session,
live movement solver or audio; this does not establish loading coverage or live
FPS. No interactive client was launched.

The capture reports zero dropped samples, trace/event rows and capacity
overflows. Detailed frames link the same `m2.light_sources` identity to both the
receiver worker and `world.lit_surfaces.consume`, exercising the new full world
presenter. Evidence remains in ignored `target/shared-light-package.*`,
`target/shared-light-world.*` and `target/shared-light-world-profile/Profiles/`.
The source validation below passed before packaging. The complete cutover remains
open; these results do not establish a performance gain.

### Shared light-source overlap checkpoint

On 2026-09-19, formatting and full workspace/all-target/all-feature Clippy passed.
The full workspace/all-feature test run passed **1,543 tests**, with **33 ignored**
and no failures across 90 suites. A controlled pending dependency proves main can
query terrain lighting from the same source bank while receiver work is waiting,
and the only worker can execute unrelated work. Eight successive frames retain
the source-bank allocation and match serial receiver output and directional order.
Separate tests cover admission refusal, failed dependencies, and a failed receiver
query after partial output, including exclusive source reuse after reclamation.

The separate-bank sunlight test retains the stock cancellation/fill expectations.
Moving M2/visibility, WMO doodad lighting/fog, terrain/liquid and renderer stock
tests passed with the split light-source interface. Logs are in ignored
`target/shared-light-final-*` files. These checks establish ownership and behavior;
they do not establish a live FPS gain, full main-ready integration or completion
of the other cutover requirements.

### Build 147 package checkpoint

Build **147** was reserved and compiled through `scripts/build-client.ps1`, then
installed through `install-test-client.ps1 -SkipBuild`. The optimized build
completed in 5m40s. Source was frozen during compilation at
`1fc8f9359b1565f83c1c074fd810ea7b3c28e481`; `dirty=true` records only the reserved
`BUILD_NUMBER`. Installed executable and package SHA-256 both match
`A39427B12637CD7EE7A3B6AF65ADD4EB000267667E4B37C58A4D935BC4176614`.

The Testing manifest, executable `--build-info`, launcher and Desktop shortcut
were verified. Settings remain 2560 x 1440 fullscreen-windowed, four CPU workers,
capacity 256, two network workers and GPU zero. F10 remains opt-in. No interactive
client was launched and no live FPS or wait-overhead gain is claimed. Validation
for the unchanged runtime source is recorded immediately below. Packaging logs
are in ignored `target/main-continuation-package.*` files.

### Resumable M2 phase checkpoint

On 2026-09-19, formatting and full workspace/all-target/all-feature Clippy passed.
The full workspace/all-feature test run passed **1,538 tests**, with **33 ignored**
and no failures across 90 suites. The M2 motion fixture compares meshes,
particles, ribbons, shadows, receiver lighting, ordering and CRT state with the
frozen serial traversal. It now checks repeated nonblocking resumes while all
workers are held, performs a hidden Vulkan operation using the released renderer
owner, and revisits a completed continuation before checking identical output.
Existing tests retain abandonment, resource-failure and attachment coverage.

New CPU tests cover non-consuming readiness with unrelated work still blocked,
stale handles, open-producer rejection without closing admission, worker wait
rejection, and failure-state recovery. Native readiness still preserves queued
gameplay input. Logs are in ignored `target/main-continuation-final-*` files.
These checks establish ordering and ownership, not a live frame-time gain or
completion of the outstanding cutover requirements.

### Build 146 package checkpoint

Build **146** was reserved and compiled through `scripts/build-client.ps1`, then
installed through `install-test-client.ps1 -SkipBuild`. The optimized build
completed in 5m18s. Source was frozen during compilation at
`213df6522b63df626a726039fc6f9c1352936470`; `dirty=true` records only the reserved
`BUILD_NUMBER`. Installed executable and package SHA-256 both match
`5C7D827DFECF8F8AD1F423E728280A41EF25BF8551079E9D2AE76A9B2E820465`.

The Testing manifest, executable `--build-info`, launcher and Desktop shortcut
were verified. Settings remain 2560 x 1440 fullscreen-windowed, four CPU workers,
capacity 256, two network workers and GPU zero. F10 remains opt-in. No interactive
client was launched and no live FPS or wait-overhead gain is claimed. Final
workspace tests passed as below; post-review formatting and workspace Clippy
also passed (`target/gpu-slot-post-review-clippy.*.log`). Package logs are in
ignored `target/gpu-slot-package.*.log`, with an exit sentinel of zero.

### Native GPU-slot checkpoint

Formatting and workspace Clippy with warnings denied passed. The full workspace
suite passed 1,534 tests, with 33 ignored across 90 suites. Five new controlled
completion tests cover 256 request generations on one worker, driver failures,
backend panic, early native error/unwind, durable notifier faults and joined
shutdown. The hidden Vulkan cinematic fixture now exercises the public slot-wait
boundary through repeated ring reuse and unchanged decoded-source identity.
Existing moving M2, native input ordering and stock GPU parity fixtures passed.

Logs are in ignored `target/gpu-slot-final-{fmt,clippy,tests}.*.log`; the final
helper exit is zero. The initial run's driver-error assertion expected Vulkan's
enum spelling rather than its Display diagnostic; the corrected test compares
the propagated operation and original driver diagnostic. No live frame-time or
ready-path overhead result is established. This connects native presentation-slot
waiting, not useful main-ready work or all external GPU operations.

Read-only follow-up confirms ordinary BLP, UI glyph and character-atlas uploads
already retain deferred transfers. Remaining direct texture host waits serve
stock generated solid/default/failure textures. Cinematic source replacement
still waits all readers, and surface rebuild/standalone M2 capacity paths retain
separate device waits. Ordinary unified-world payload growth already waits only
its selected slot; do not treat all buffer growth as a device-wide drain.

### Build 145 package checkpoint

Build **145** was reserved and compiled through `scripts/build-client.ps1`, then
installed with `scripts/install-test-client.ps1 -SkipBuild`. Its executable source
identity is `1737172aea09664efc10af2563f9a4322b9a1fc3`; `dirty=true` records the
pending BUILD_NUMBER reservation, the only tracked change during packaging.
The optimized build completed successfully in 5m 43s.

Artifact and installed SHA-256 both equal
`B0A04E4779AC32F84B0E98FA7EEEE8994735F30E109EDE461699FEBD87ABFCE0`.
The Testing shortcut uses the installed launcher, retaining 2560x1440
fullscreen-windowed, four CPU workers, capacity 256, two network workers and
GPU zero. F10 remains opt-in. No live client was launched for this package;
no measured FPS gain or complete CPU cutover is claimed.
Logs are in ignored `target/frame-native-wait-package.stdout.log` and
`target/frame-native-wait-package.stderr.log`.

### Native frame-consumption checkpoint

On 2026-09-19, formatting, full workspace Clippy with warnings denied and all
1,529 workspace Rust tests passed, with 33 ignored across 90 suites. The new
native-readiness fixture releases a blocked CPU result after observing the wake
ticket, preserves a queued quit event until ordinary input dispatch, consumes
the result exactly once and verifies reclaimed ownership and predicate-error
propagation. Existing native wake race/fault fixtures also pass.

The moving geometry comparison now uses the live native wait context and its
coordinator notifier. It still matches the frozen serial traversal's meshes,
palettes, particles, ribbons, shadows, scene lighting, ordering, RNG and retained
state across motion and visibility changes, including recovery after a resource
mismatch. Other scene fixtures explicitly retain offline executor consumption.
Ready geometry consumption uses one batch result lookup and does not call SDL;
pending waits keep named F10 spans separate from useful preparation.

Logs are in ignored `target/frame-native-wait-checked-{fmt,clippy,tests}` with
`.stdout.log` and `.stderr.log` suffixes. This connects native service at M2 CPU
consumption boundaries, not general main-ready continuations or GPU-slot waits.
It does not establish a live FPS gain or complete the architecture cutover.

### Build 144 package checkpoint

Build **144** was reserved and compiled through `scripts/build-client.ps1`, then
installed with `scripts/install-test-client.ps1 -SkipBuild`. The Testing shortcut
points at the installed launcher. Its executable source identity is
`ec5098bc63bb2c3f71714fd0ce511cb000298119`; `dirty=true` records the pending
BUILD_NUMBER reservation, which was the only tracked change during packaging.
The optimized build completed successfully in 4m 52s.

Artifact and installed SHA-256 both equal
`B673280EB204C7EA8AF2C848B47635415DCB65E76E4CA7E8DF3E06BAF623B712`.
The installed launcher retains 2560x1440 fullscreen-windowed, four CPU workers,
capacity 256, two network workers and GPU zero. F10 remains opt-in. No live client
was launched for this package and no frame-rate gain is established.
Packaging logs are in ignored `target/population-readiness-package.stdout.log`
and `target/population-readiness-package.stderr.log`.

### Shared population primary-source checkpoint

On 2026-09-19, formatting, full workspace Clippy with warnings denied and all
1,528 workspace Rust tests passed, with 33 ignored across 90 suites. A controlled
source owner holds publication while both NPC and remote-player consumers join;
useful work still executes on the only worker. Both published residents retain
the exact same source lease. The fixtures also cover saturated dependency
admission, object withdrawal and recycled-identity rejoin, abandoned-source error
fan-out followed by a new successful request, complete mounts and current motion
at publication. A separate controlled completion verifies that a withdrawn
attempt returns its cache and discards cancellation even while its object remains
current. This prevents appearance changes from reviving obsolete attempts.

Final logs are in ignored `target/population-readiness-checked-{fmt,clippy,tests}`
with `.stdout.log` and `.stderr.log` suffixes. The preceding full run passed 1,527
tests before adding terminal-withdrawal handling; the final run covers that
correction. These checks establish request ownership and publication behavior,
not a measured live frame-rate improvement. Nested appearance sources and the
remaining cutover requirements are still open.

### Build 143 package checkpoint

Build **143** was reserved and compiled through `scripts/build-client.ps1`, then
installed with `scripts/install-test-client.ps1 -SkipBuild`. The Testing shortcut
points at the installed executable. Its source identity is
`4d9cc31d4033f39cb22c9f867a31ab5df384e685`; `dirty=true` records the pending
BUILD_NUMBER reservation, which was the only tracked change during packaging.
The optimized build completed successfully in 4m 35s.

Artifact and installed SHA-256 both equal
`E3027E858229A28D9A461F05CF12B32924FCD0B503FC8175BFE03A5D1DAD03CE`.
The installed launcher retains 2560x1440 fullscreen-windowed, four CPU workers,
capacity 256, two network workers and GPU zero. F10 remains opt-in. No live client
was launched for this package and no frame-rate gain is established.
Packaging logs are in ignored `target/m2-readiness-package.stdout.log` and
`target/m2-readiness-package.stderr.log`.

### Root-pose readiness checkpoint

On 2026-09-19, formatting, full workspace Clippy with warnings denied and all
1,525 workspace Rust tests passed, with 33 ignored across 90 suites. Five Python
trace-analysis tests passed. A controlled unit fixture holds every worker until
main admission returns, checks that the root callback sample remains unconsumed,
then exercises both resumed publication and abandonment followed by another frame.
The moving serial-reference fixture includes a unit root midway through ordinary
placements, so earlier geometry already owns effect state when traversal pauses.
It compares meshes, palettes, shadows, particles, ribbons, receiver lighting,
ordering, RNG and simulation state across motion and visibility changes.
The existing WMO clip/fog and pixel fixtures also pass.

The synthetic unit requires authored stand-turn clips and matching AnimationData
rows; the fixture now supplies both. Production missing-animation behavior is
unchanged. Trace analysis joins early pose workers to resumed placement/source
records through one logical scene identity and still reads historical captures.

Final logs are in ignored `target/m2-readiness-complete-{fmt,clippy,tests}.stdout.log`
and `.stderr.log`; the same prefix's `targeted` log records the focused moving
comparison. This verifies scheduling and parity, not a live FPS improvement.
The full continuation/native-wait and resource cutover requirements remain above.

### Build 142 package checkpoint

On 2026-09-19, the optimized `test-client` package compiled successfully and was
installed through the existing Testing shortcut. The executable reports version
`0.0.3a`, build `142`, source `68b3f19f65d1d5222f84d6289799ad3c1e52c1f4`, and
`dirty=true`; only the build-number reservation was uncommitted during compilation.
The installed executable matches the packaged artifact's SHA-256:
`A9C6CB746EB7E9FC248A6DCFE01DE52B332FDBBBA28D7ACE402DD05255FFD96A`.

The shortcut and launcher were verified at 2560x1440 fullscreen-windowed, four
CPU workers, CPU capacity 256, two network workers and GPU index 0. F10 remains
opt-in, and the client was not launched. This is a tested, connected source
checkpoint; no live FPS improvement or full cutover completion is established.
Packaging logs are in ignored `target/frame-continuation-package.stdout.log`
and `target/frame-continuation-package.stderr.log`.

### Independent world preparation checkpoint

On 2026-09-19, formatting, workspace Clippy with warnings denied and the complete
workspace suite passed: 1,525 tests, zero failures and 33 ignored across 90 suites.
All four trace-analyzer tests also passed, covering old and staged scene labels,
source identity and deferred geometry-worker attribution.

The moving geometry fixture now occupies every CPU worker before M2 admission.
Admission returns before those workers are released, then final meshes, palettes,
particles, ribbons, lighting, RNG state and ordering match the frozen serial path.
Dropping another admitted frame restores all model-owned effect state. A separate
static/moving WMO pixel fixture verifies unchanged exterior clips, ordered group
frusta and fog selection before and after M2 receiver completion. Existing mount,
vehicle, shadow, liquid and interior-lighting checks passed in the full suite.

Logs are in ignored `target/frame-continuation-{fmt,clippy,tests}.stdout.log` and
matching `.stderr.log` files. This proves the connected staging/ownership
contracts; it establishes no live FPS gain and leaves the remaining complete
cutover requirements open.

### Build 141 package checkpoint

On 2026-09-19, the optimized `test-client` package compiled successfully and was
installed through the existing Testing shortcut. The installed executable and
Cargo artifact have matching SHA-256:
`C3CE37A1A0FF65BDB576B0213FF2A703D94379B9B0EED70C069051B953BF1CEB`.
The executable reports version `0.0.3a`, build `141`, source
`45d6ab020e22205c2f626e1904549843391472ed`, and `dirty=true`; the build-number
reservation was the only tracked change during compilation. Installation reused
that artifact identity without reserving another number.

The launcher retains 2560x1440 fullscreen-windowed presentation, four CPU workers,
CPU capacity 256, two network workers and GPU index 0. F10 detail tracing remains
opt-in. The client was not launched, so this package establishes availability,
not live performance or gameplay verification. Source validation is recorded in
the resource-gated loading checkpoint below.

Build 140 was reserved before the reported Codex crash and VS Code restart. Its
build process disappeared without producing an executable; the number remains
an incomplete attempt. Build 141 reused the compiled dependencies and completed.
Persistent packaging logs are in ignored `target/build141-package.stdout.log`
and `target/build141-package.stderr.log`.

### Resource-gated loading checkpoint

On 2026-09-19, formatting, workspace Clippy and the full workspace suite passed:
1,525 tests passed, none failed, and 33 were ignored across 90 suites. Coverage
includes single-worker progress behind a resource gate, shared admission,
protected-worker exclusion under inherited urgency, service turns between load
kernels, cancellation before execution, shutdown, stale service controls and
source-publication races. The warmed graph fixture still records zero allocator
calls across 1,000 frame activations.

The runtime fixture now admits the real GameObject dependency before source
publication and observes completed preparation without another coordinator poll.
It removes the last object, rejoins under a replacement lifetime, and verifies
that withdrawal/failure returns the same mounted cache bank. A separate priority
fixture verifies that retirement never directly demotes a producer shared with
another required consumer.

Logs are in ignored `target/load-graph-final-tests.log`,
`target/load-graph-package-clippy.log` and `target/load-graph-final-fmt.log`.
The resumed environment required restoring Ninja on PATH and using a shorter
Cargo output-path alias to the same target directory for MSVC's native object
path limit. No dependency versions or source behavior were changed for that fix.
These checks establish ordering and ownership, not a measured FPS improvement
or completion of the remaining cutover requirements.

### Epoch registration ownership checkpoint

Formatting, workspace Clippy and the full workspace suite pass: 1,515 tests,
zero failures and 33 ignored. All 66 CPU tests pass. A controlled regression holds
one real batch's metadata lock while another thread admits an independent epoch;
the new admission completes before the held lock is released. Existing shutdown,
readiness, priority, storage and reclamation checks pass. The allocator fixture
still observes zero allocator calls across 1,000 warmed graph activations.

Logs are in ignored `target/epoch-registration-cpu-tests.log`,
`target/epoch-registration-clippy.log` and
`target/epoch-registration-workspace-tests.log`. This removes a cross-batch lock
dependency and an owner-disposal hazard; it does not establish a live FPS gain.
Build 139 remains installed, and the full architecture cutover remains open.

### Resumable terrain service checkpoint

The full workspace suite passed 1,514 tests, with zero failures and 33 ignored.
The focused runtime terrain run passed 135 tests with 12 ignored. New coverage
checks service interleaving before complete generation publication and reuse of
the same archive bank after a texture failure. Existing terrain/camera-window,
GPU-admission, duplicate-placement and movement-reference checks also pass.

Six terrain test sites previously treated an empty queued job as a fence for
an entire preceding load. Their isolated service fixture now waits for outstanding
admission to drain before fencing terminal publication. This preserves the exact
completed-versus-staged races under the resumable scheduler. Separately, two CPU
storage assertions now join terminal worker epilogues before checking final
release: input reclamation does not guarantee the runner's last shared reference
has already been dropped. Production disposal remains nonblocking.

Logs are in ignored `target/terrain-steps-workspace-tests-verified.log`,
`target/terrain-steps-terrain-tests.log`, `target/terrain-steps-storage-tests.log`
and `target/terrain-steps-clippy-final.log`. Formatting and workspace Clippy pass.
Build 139 remains the installed Testing artifact; this checkpoint creates no new
numbered package and establishes no live FPS improvement.

### Owned appearance workers and Build 139 checkpoint

On 2026-09-15, final formatting, workspace Clippy and the full workspace suite
passed: 1,512 tests, zero failures, 33 ignored. New controlled worker tests verify
that selected appearance churn yields to queued service, skips superseded input,
preserves current failure identity and stops on request withdrawal. Existing
population admission, equipment, animation, movement and stock rendering checks
also passed after the cache bank moved to explicit task/completion ownership.

Build 139 was compiled with the optimized `test-client` profile and installed
through the existing Testing shortcut at 2560x1440. Its recorded source identity
is `5e7d0b7b` with uncommitted changes, captured by this checkpoint; installation
did not relabel the artifact. Installed SHA-256:
`30F13F896B406C46D60159529A41A35030997782D45B4295BBAEDE1A5A772E31`.
Build 138 was reserved by an aborted packaging invocation; the sequence retains
that gap as required. The user reported no visible FPS improvement in Build 139.
Validation compilation overlapped that run, so it was not a controlled comparison.
No measured performance gain is established and the complete cutover remains open.

Final validation logs are in ignored `target/build139-final-tests.log`,
`target/build139-final-clippy.log` and `target/build139-final-fmt.log`.

### Shared M2 requests and consumer priority checkpoint

On 2026-09-15, final formatting, workspace Clippy and the complete workspace
suite passed: 1,509 tests, zero failures, 33 ignored. New real archive/model
fixtures verify one published source across independent consumers and legacy
path aliases, shared original errors, namespace rejection, abandoned producer
completion and rejection of unfinished CPU-worker waits.

Controlled CPU tests verify promotion of the original queued producer and
priority withdrawal when the last required consumer leaves. Their speculative
producer admission preserves the reserved required-work slot. Runtime coverage
joins pending sources with every CPU slot occupied, then checks independent
backdrop publication and GameObject withdrawal/re-entry against exact shared
source identity. Existing stock animation, rendering, visibility, movement and
collision fixtures passed in the full run.

Final logs are in ignored `target/shared-m2-final-tests.log`,
`target/shared-m2-final-clippy.log` and `target/shared-m2-final-fmt.log`.
Superseded generated debug symbols were removed to restore validation disk
space. This checkpoint connects Glue backdrops and asynchronous top-level M2
GameObjects; the source domains listed above still require cutover. It does not
establish live FPS gains, whole-frame memory coverage or a numbered Testing build.

### Qualified M2 retention and CPU maintenance checkpoint

On 2026-09-15, final formatting, workspace Clippy and the full workspace suite
passed: 1,501 tests, zero failures, 33 ignored. New coverage checks the native
10,000 ms boundary, unsigned wrap with signed comparison, a delayed old-pin
release after reacquisition, ordered expiry after middle-entry reacquisition,
and idle retirement after pool saturation clears. Real archive/model fixtures
also verify shared catalog registration, independent rediscovery namespaces,
closed cache cleanup and preservation of live consumers.

The timed final-release path, including its durable maintenance signal, records
zero allocator calls. The earlier warm lease/cache-hit allocation checks still
pass. This is not a measurement of whole-frame overhead or live FPS. CPU service
steps detach at most sixteen expired sources or one closed cache owner per turn;
a single source/cache destructor remains indivisible. Complete resource byte
accounting and pending-load sharing remain required.

Final logs are in ignored `target/m2-retention-tests-final.log`,
`target/m2-retention-clippy-final.log` and `target/m2-retention-fmt-final.log`.
No new numbered Testing package or live performance improvement is established.

### Model resource leases checkpoint

On 2026-09-15, formatting and workspace Clippy passed. The complete workspace
suite passed 1,495 tests with 33 ignored, including model cache identity and
collection, movement/collision, M2 animation/geometry and renderer stock fixtures.
Six focused lifetime tests passed separately. They cover stale notification
generations, release/reacquire identity, consumer survival after cache teardown,
and destruction outside the release metadata lock.

Allocation instrumentation observed zero allocations for 1,000 warm lease-clone
and cache-hit cycles followed by final release/collection. A separate worker
final-release window also observed zero allocations. Initial cache registration,
decoding and reacquisition after every consumer has left remain cold allocation
boundaries; these tests do not establish whole-frame allocation or FPS results.
The native retention follow-up above separately passed 112 original-instruction
cases. No numbered Testing package was produced. Logs remain in ignored
`target/resource-leases-focused.log`, `target/resource-leases-clippy.log`,
`target/resource-leases-workspace-tests.log` and
`target/model-cache-qualification.json`.

### Shared pending requests and audio checkpoint

On 2026-09-15, formatting and full workspace Clippy passed. The full workspace
suite passed 1,489 tests with 33 ignored. Eight new tests cover one producer per
key, identical shared payload/error ownership, late joining, direct demand
promotion/withdrawal, capacity refusal before input transfer, abandoned producer
drain, worker panic recovery, nonadjacent A/B/A audio completion order, independent
voice cancellation and the engine-owned pending signal. The focused request and
runtime audio suites also passed before the final full validation.

The final suite includes the F10 request/join/consume/release provenance links.
These checks establish request ownership and ordered audio behavior; they do not
establish full cache byte accounting, cross-resource dependency integration,
profiling overhead or a live FPS improvement. No numbered Testing package was
created. Logs remain in ignored `target/shared-requests-clippy-final.log`,
`target/shared-requests-workspace-tests.log`, `target/shared-requests-focused.log`
and `target/shared-sound-focused-final.log`.

### Archive service boundaries checkpoint

The full workspace suite passed 1,481 tests with 33 ignored. Workspace Clippy
with warnings denied and formatting passed. New coverage verifies one archive
per advance, no partial-stack publication, unchanged first-error order, namespace
and precedence preservation, domain steps after completed mounting, and ordered
texture failures with canonical alias reuse. Existing world UI source ownership,
M2 motion/visibility, population replacement and shutdown checks also passed.
Logs: `target/archive-steps-tests.log` and `target/archive-steps-clippy.log`.

A hidden installed-asset debug replay completed 336 frames: 48 each of streaming,
stationary, orbit, pointer, outbound, return and settled phases. It used Soap's
saved appearance, map 1 at (1515.34, -4417.27, 18.0499), travel offset (18, 0, 0),
2560x1440, four CPU workers, farclip 1277, environmentDetail 1.5, extShadowQuality
5 and an isolated profile with audio/VSync disabled. It exercised 46?330 M2
packets and 3,532?11,208 particle vertices per frame, but stayed on one resident
tile with zero tile admissions/evictions. It therefore checks primary-scene
execution only, not neighbor loading, a server population, live sliced UI startup
or the movement solver. Debug/hidden timings are not FPS evidence.

The first launch omitted mandatory runtime options and stopped at validation.
After fixing the invocation, the ordinary debug example overflowed its 1 MiB main
stack before startup. An ignored copy of that same executable with only its PE
stack reserve raised to 32 MiB completed the replay; this matches the existing
large-stack diagnostic convention without changing client source or the installed
Testing executable. The source of that debug stack requirement and optimized
client execution remain unverified. Evidence: `target/cpu-cutover-archive-smoke.csv`,
`target/cpu-cutover-archive-smoke-summary.json`,
`target/cpu-cutover-archive-smoke-stack32.log` and the earlier
`target/cpu-cutover-archive-smoke.log`. No new numbered Testing build is claimed.

### Resumable service checkpoint

The full workspace suite passed 1,476 tests with 33 ignored. Workspace Clippy
with warnings denied and formatting passed. Controlled tests cover priority
promotion/withdrawal during a running step, required loading between retirement
turns, forward exactly-once destruction, panic publication after capture retirement,
and shutdown drain after the consumer discards its handle. A separate allocation
fixture observes zero allocation calls across 1,000 warmed resumptions.

The first full test compile ran out of disk while linking. After the failed
process terminated, only generated incremental caches were removed; the complete
retry passed with incremental compilation disabled and two build jobs. Logs are
in ignored `target/cpu-steps-workspace-tests-initial.log`,
`target/cpu-steps-workspace-tests.log` and `target/cpu-steps-clippy.log`.
This connects finite cleanup steps, not calibrated time slicing or resumable
archive/codec work. No live performance gain or Testing build is claimed.

### Archive namespace checkpoint

The full workspace suite passed 1,470 tests with 33 ignored. Final workspace
Clippy with warnings denied and formatting passed. The suite covers M2 alias
reuse across cloned mount plans, isolation across rediscovery/different bytes,
missing-source errors, namespace-filtered texture merge/upload enumeration,
WMO collection, font coverage reuse, and distinct decoded audio sample lifetimes.
The final edit shortened the main asset borrow before UI GPU prewarm; it copies
only the same namespace value. Final Clippy checks that source.

Logs are in ignored `target/asset-namespace-workspace-tests.log` and
`target/asset-namespace-clippy-final.log`. Independent caches still have their
existing ownership and retention policy. No shared pending-request authority,
cache byte budget, live FPS gain, or numbered Testing build is claimed here.

### Background demand checkpoint

The full workspace suite passed 1,467 tests with 33 ignored. Workspace Clippy
with warnings denied and formatting passed. The full test compile preceded a
parentheses-only warning cleanup; Clippy checks that final source. New controlled
CPU tests cover required/retirement alternation, speculative admission headroom,
promotion and withdrawal without duplicate execution, and frames ahead of
speculation. Existing single-worker alternation, protected-worker exclusion,
moving M2 parity and terrain ownership/publication checks also passed.

Logs are in ignored `target/cpu-service-workspace-tests.log` and
`target/cpu-service-clippy.log`. These validate ordering and lifetime contracts;
no new live FPS result or numbered Testing build is established. Concrete shared
asset requests and bounded resumable bulk/retirement work remain required.

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
