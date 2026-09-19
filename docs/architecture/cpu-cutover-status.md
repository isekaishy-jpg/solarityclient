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

## Connected source changes

- World presentation now uses a scoped M2 admission/publication boundary. Scene
  callbacks and traversal launch owned geometry, then seal its producer before
  returning to main. Ground-detail selection/publication and WMO packet creation
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
  overlapping work is not counted as M2 execution. This first world continuation
  does not remove per-model pose waits during traversal or the remaining final
  consumption barriers; the full continuation/native-wait integration remains
  required below.

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
  wrapper shares only selected request metadata instead of a mutex-protected
  archive/cache bank. Model caches retain their existing metadata synchronization.
- Coalesced Glue character preparation returns to the CPU service queue after each
  obsolete appearance attempt. It re-reads current demand on the next turn instead
  of looping through replacements inside one indivisible task. Current failures
  retain their selected key; withdrawal ends the continuation. Worker ownership,
  source preparation and coalescing policy now have focused folder modules.
  One complete appearance attempt remains indivisible; nested model dependencies
  and shared source adoption for these consumers remain required.
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

## Still required for the complete cutover

- Typed shared-result leases across domains and main-only continuations.
  Templates, heterogeneous phase fan-in and frame urgency propagation now exist; resource
  cache/I/O integration still requires its complete concrete dependency graphs.
- Calibrated cost buckets, straggler reporting and measured step-size policy;
  resumable asset/bulk stages beyond retirement, the connected archive mounts,
  terrain stages and Glue texture steps, and
  remaining domain demand transitions beyond terrain/Glue prewarm. External producers
  expose urgency, but those services must still consume it. Frame urgency is
  monotonic within an epoch. Shared M2 primary-request consumers now support live
  priority withdrawal; remaining source domains still need that connection.
- Connect reservations to allocations nested inside domain job state and the
  complete required phase working set. Executor-wide scheduler metadata and typed
  result-page accounting now exist; model output and override buffers now adopt it. Live simulation/pose storage,
  final frame streams, ordinary asset buffers and caches still require adoption,
  connected working-set admission, explicit trimming and maintenance policy.
- Extend the native bridge to loading/GPU-slot waits and main-ready continuations.
  Current frame consumers still wait at their necessary consumption boundaries.
- Cross-domain terrain/WMO/UI/rendering overlap and phase-specific M2 demand;
  ordered receiver lighting and end-of-frame state reclamation still have barriers.
- Extend pending-request authority beyond runtime audio, Glue primary M2s and
  asynchronous top-level GameObject M2s. Terrain, population/appearance, nested WMO
  doodads, effects and sky sources still use their existing local decode caches.
  WMO and other source domains need shared pending authority, cross-resource I/O
  dependencies and the remaining domain-wide shared result leases. M2/WMO sources
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
Build 142 was packaged and installed on 2026-09-19 from `68b3f19f`. It includes
the connected independent world preparation boundary on top of Build 141's
resource-gated loading, epoch registration and resumable terrain changes.
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
