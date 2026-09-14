# Build 133–134 performance fixes

This work closes the measured issues from captures `1789368498983-1` and
`1789373560190-1`. It preserves stock event ordering, visibility, animation,
shadow consumers and collision results. Loading latency, moving-scene hitches
and sustained frame time require separate verification.

## Completion requirements

- [x] World-entry UI: remove repeated broad presentation reconstruction between
  initialization events, preserve script-visible geometry, and address the
  synchronous 2.45-second initialization stall. Exercise real FrameXML loading.
- [x] Frame-critical CPU batches: resolve the dependency behind the 75 ms pose
  join; isolate frame work from background loading without discarding required
  poses or introducing detached work. Validate under occupied loader workers.
- [x] M2 preparation: establish explicit consumer requirements and retained
  preparation across animation, shadows, effects and packets; validate reduced
  work and unchanged output using representative city consumers.
- [x] Placement topology: ordinary dynamic membership changes must not rebuild
  immutable scenery metadata. Cover additions, retirement and source remapping.
- [x] GameObject publication and collision: retain owner membership, suppress
  unchanged publication and bound re-registration to affected geometry. Preserve
  moving transports, tile readiness and native candidate ordering.
- [x] Validate the world-service action/event spike through the shared UI update
  changes; distinguish genuine work from fence/limiter/scheduling waits.
- [x] Run required formatting, workspace Clippy and tests; record targeted
  measurements, commit/push completed work, and finish with an installed numbered
  Testing build. Do not claim a matched FPS gain from unmatched route windows.

## Evidence and progress

Starting point: Build 134, source `042fe0d3`, package commit `c6aafb8f`.
Both captures identify repeated full-scene invalidation; Build 134 also records
unchanged GameObject projection inputs with repeated collision registration,
long low-cycle pose joins, and repeated 140–160 ms world-entry UI snapshots.
All sampled batch results were consumed. Particle animation behavior is deferred
at the user's request; no visibility bug or unused-pose explanation is assumed.

Implementation and validation results will be recorded below as completed.

### Isolated frame scheduler

The configured worker budget is now partitioned between joined frame work and
background jobs (four configured workers means two of each). Frame joins never
enter the archive scheduler and do not consume background admission capacity.
The one-worker case executes frame work on the caller. Shutdown still owns all
workers, results preserve input order, and no jobs or poses are discarded.

Committed as `c9e87500`. The CPU suite passes 13 tests, including a regression that blocks every archive
worker and exhausts job capacity, then requires the frame batch to finish before
those jobs are released. Panic, shutdown and one-worker behavior also pass;
CPU Clippy passes with warnings denied. The runtime pose consumer uses the new
boundary. Workspace Clippy and all-feature tests pass, including scalar-equivalent
runtime palettes. Matched live timing remains pending.

### World-entry event publication

FrameManager now exposes a nested presentation transaction. Event callbacks
remain immediate and ordered; live Lua bounds/dimensions remain queryable before
native publication. Full-refresh layout callbacks, including scroll-range
notifications, remain at their original event boundaries. The transaction merges
mutation journals and publishes one native snapshot/glyph/draw generation at its
outer boundary. World-entry notifications use that boundary under the existing
stock sound-admission scope.

Focused tests compare final geometry with ordinary per-event publication, query
dimensions between callbacks, create/reparent regions, exercise nesting, and
restore publication after an unwind. This removes repeated startup presentation
work; initial manifest/static-plan/Lua construction and live loading measurements
remain to address before the loading requirement is complete. Action-slot event
batches use the same boundary. The complete publication remains inside the
native world-entry sound-suppression scope, including final layout callbacks.

### Event subscription ownership

The full-stock test exposed a separate broad traversal: every event inspected
all 25,099 live Lua regions to discover its subscribers. Presentation batching
alone left 144 action-slot notifications at approximately 3.2 seconds in the
debug diagnostic. Event subscriptions now own ordered explicit/all-event sets.
Dispatch visits their union and re-queries after each callback, preserving later
registrations/removals, duplicate suppression, and the entry-time object limit.
The AddOn callback path shares the index. Empty event buckets retire immediately.

Full-stock debug diagnostic observations on this machine (not FPS measurements):

| Same 25,099-region workload | Arena scan | Indexed subscriptions |
|---|---:|---:|
| Sequential 144 action events | 3,222 ms | 95 ms |
| Batched 144 action events | 3,187 ms | 90 ms |
| Sequential entry events | 2,313 ms | 2,116 ms |
| Batched entry events | 1,557 ms | 1,372 ms |

The full-stock regression passes: all region visibility, alpha, scale and
animation state agree, layout bounds agree within 1e-8 UI units, and ordered
texture presentation packets agree exactly. The tolerance covers f64 rounding
from repeated dimension publication, far below the f32 drawing ABI. Initial
construction remains approximately 5.8 seconds in this **debug** test; optimized
build loading and the specific captured world-service spike still need validation.

The UI suite passes 28 library tests and 165 integration tests, with the full-stock
test run separately. A new 2,048-unrelated-frame regression covers registration of
later and earlier owners during dispatch, removal of later subscribers, newly
created owners, duplicate/all-event admission, and complete unregistration.
Workspace Clippy passes with warnings denied after these changes. The complete
workspace run after the event index and final sound-scope edit also passes;
the intermediate Build 135 package records that verification below.

### Collision registration dependencies

Generic GameObjects and transport map models retain the root revision observed
by their last registration. A moved WMO no longer globally invalidates their
destination lists. Unit probes use the collision-center column; render-box
registration tests the previous and current inverse root matrices using the same
local AABB rule as stock. The latter is intentionally more permissive than world
box intersection for rotated roots. Source/tile topology changes and expired
dependency history still execute the original registration query.

Committed as `02e45a00`. Tests cover arrival/departure, inclusive contacts, distant roots, a collision
center outside its render box, and the rotated-root over-admission case. Full
runtime workspace checks pass; matched live measurements remain pending. GameObject source
publication polling is a separate unfinished part of this requirement.

### Intermediate Testing build 135

Installed on 2026-09-14 from source `9040f672fff09b552f4fa5c1eda47117d477e0e8`,
including the scheduler, collision dependency and UI event/publication commits.
Formatting, workspace all-target/all-feature Clippy with warnings denied, and
workspace all-feature tests pass: 1,403 passed, zero failed, 31 ignored. The
full-stock UI comparison above was run separately. The official package script
reserved Build 135 and completed the optimized `test-client` build.

The installed executable reports product `0.0.3a`, build `135`, and that source
revision. Its dirty flag reflects the reserved BUILD_NUMBER change. Installed
and packaged SHA-256 hashes both equal
`3942BC182A28E35B2685453D43B7EBD43F3926FEA23106CFE025292CB12B63AE`.
Installation retains the 2560x1440 Testing launcher and normal F10 activation.

This is an intermediate package, not completion of the performance goal. Live
loading/frame-time improvements have not been measured for this executable;
M2 preparation, ordinary placement topology and the other unchecked requirements
remain in progress. Work continues without waiting for a user-run test.

### Retained scenery publication

Ordered placement storage now carries compact source references, static/effect
classification and previous publication indices through removal, extraction and
stable effect ordering. Dynamic owner filtering does not read scenery simulation
records. Character component retention extracts only the selected components;
it no longer drains and reconstructs the entire scenery vector.

Visibility publication builds immutable scenery facts once per placement lifetime
and retains them across ordinary dynamic changes. Dynamic ancestry and attachment
requests use dynamic membership. Index relocation remaps spatial leaf references
without rebuilding their unchanged partition nodes. A static addition/removal
still performs the required spatial membership rebuild. Reusing an owner key at a
different transform creates new metadata, rather than inheriting a retired owner.

Source liveness comes directly from current compact storage even before topology
publication. Source compaction writes only relocated simulation slots and updates
cached static references, including multiple compactions before publication.
This replaces a dirty-cache full simulation traversal and avoids writing every
instance when an unused source suffix retires. Cache storage remains bounded by
current/pending publication capacities; it does not retain GPU resource generations.

The former full metadata traversal is retained in the test tree as an independent
oracle. A 26,000-scenery fixture interleaves dynamics, removes and re-adds mounts,
compacts sources repeatedly, replaces a static owner at a new transform, and
empties the scene. All ancestry/admission fields and camera/shadow selections
agree. Existing camera, lighting, equipment, retirement and effect lifecycle
regressions also pass. A fixture that previously changed a live object's static
classification in place now transfers it through the storage admission boundary;
its visibility assertions remain unchanged.

Formatting, workspace all-target/all-feature Clippy with warnings denied, and
all-feature workspace tests pass: 1,404 passed, zero failed, 32 ignored. The
manual optimized publication comparison also passes. Across 40 forced
publications of the same 26,028-placement fixture, the former metadata traversal
alone averages 2.675855 ms; the complete incremental topology publication averages
1.112025 ms. Both measurements include stock worker-prepared static spatial data.
This isolates publication CPU work; it is not a live frame-time or FPS comparison,
and it does not measure the physical vector compaction before publication.
F10 now records newly built static metadata
and actual spatial rebuild membership independently of total published placements.
This source work follows Build 135 and is not in that installed executable.


### World UI archive preparation

The live world-entry path now admits one bounded background job after the loading
card has been presented. The job mounts the fixed archive catalog, loads localized
spell names and default bindings, expands the FrameXML manifest and validates Lua
syntax. It returns owned declarations; the validator Lua state is destroyed on its
worker. No authored Lua callback, live character handle or GPU resource crosses
threads. Main-thread construction consumes the result only with current authoritative
world facts. Validation failures are observed at that admission boundary, preserving
the earlier login/metadata error order. Synchronous diagnostic construction remains
available as a comparison.

Saturation leaves admission pending and never runs archive work on the frame caller.
The owner polls completion without joining unfinished work. A cancelled entry can
retain one completed character-independent source image for the next entry; shutdown
observes any outstanding result/error before closing the CPU pool. There is no cache
of historical UI generations. Frame-manager loading and runtime construction have
been separated from their live update implementations into focused child modules.

`ui.source.load`, `ui.frame.source_preparation` and
`runtime.world_ui.source_preparation` record the archive phases on their executing
thread. `ui.startup` now measures main-thread construction after declarations are
available, so its total excludes the former `bundle` phase. Compare source and
construction costs separately when comparing against Build 134.

The automated UI suite passes 28 library and 168 integration tests. New cases prove
that valid Lua which would throw at execution still prepares on a foreign thread,
invalid Lua retains its typed error and normalized source path, and prepared
construction matches synchronous geometry. The sound-admission regression now
crosses the worker boundary. The full stock comparison also crosses that boundary
and matches all 25,099 regions and ordered presentation packets after world-entry
and action events.

A debug observation includes 1,593 ms of worker source preparation and 5,363 ms of
remaining main-thread construction. Its total 6,956 ms is **not** a loading-speed
improvement claim (the synchronous pass was 5,946 ms). This change relocates and
allows overlap of source preparation; static planning, Lua execution and first
presentation remain synchronous and the loading completion requirement stays open.
The optimized `test-client` comparison also passes. Its source job (including
thread startup and archive mount in this diagnostic) took 181.334 ms, and the
remaining main-thread construction took 1,054.330 ms. The sequential pass took
1,389.072 ms total; the prepared pass took 1,235.664 ms total. These are one-pass
fixture observations, not a matched live loading or FPS result. In particular,
the remaining approximately one-second main-thread block still needs attention.

Final formatting, workspace all-target/all-feature Clippy with warnings denied,
and workspace all-feature tests pass: 1,409 passed, zero failed, 32 ignored. This
includes the admission-order correction and source-job capacity/retirement tests.
The optimized stock comparison ran separately and passed. No additional numbered
package has been reserved after Build 135; the goal remains active.


### Cooperative world UI construction and entry publication

The remaining FrameXML constructor now owns a local, pinned construction task.
Runtime advances it behind the loading card, preserving source order and whole
Lua callbacks. The caller retains one task, never exposes a partial FrameManager,
and drops all partial plans and sound guards on cancellation. Lua and live world
handles stay on the main thread. The source worker remains separate.

World-entry events use the same ownership boundary. Their callbacks run in the
original order, retain immediate Lua geometry and scroll-range callbacks, and
merge native presentation journals before one final publication. Session
publication stays held until construction and entry complete, just as it was
held by the old synchronous call. Network workers retain their existing bounded
queues; packets are not consumed and discarded while the UI is incomplete.
Loading-card presentation, platform input, close handling and F10 remain serviced.

Full Lua snapshots now copy the ordered arena in 64-object groups. Ordinary live
snapshots and refreshes use the same implementation synchronously, without a
heap task allocation or an active deadline. Live refresh retains its existing F10
phase names. Covered startup/entry records active poll work and excludes time
between polls. Construction, event publication, snapshot copying and full refresh
have focused modules; runtime state now has a folder-backed module.

The live loading allowance is 8 ms per advance, checked between operations. It
is deliberately larger than the 2 ms stress budget in the stock fixture so load
completion does not require hundreds of separately paced loading-card frames.
A callback or unsplit native operation can exceed that allowance: this is not a
hard frame deadline. Total loading latency under VSync still needs measurement.
GPU admission, native runtime initialization and some initial plans remain
indivisible, and this work does not establish a live loading-speed or FPS claim.

Glyph request construction previously hashed the common 224-character range for
every text owner. It now requests that range once for each distinct font while
preserving actual required characters, optional coverage and sorted glyph order.
The full-stock optimized diagnostic used 17 distinct font keys. The glyph phase
fell from 157.986 ms to 48.987 ms in successive fixture observations. This removes
redundant CPU work; it does not change glyph rendering or require new cache
lifetimes. Total construction varied between runs, so that phase measurement
must not be reported as an equivalent reduction in overall loading time.

The stock fixture compares the synchronous path with worker source preparation,
sliced construction, sliced entry callbacks and sliced final publication. It
checks all 25,099 regions and ordered presentation packets after entry and 144
action-slot notifications. Focused tests cover callback-time created geometry,
scroll-range event ordering, source-order construction, and sound admission on
completion or cancellation. Final formatting, workspace all-target/all-feature Clippy with warnings denied,
and all-feature workspace tests pass: 1,411 passed, zero failed, 32 ignored.
The two focused construction ownership tests also passed separately.


The final optimized stock comparison passes. With its stricter 2 ms stress budget,
construction took 1,017.841 ms of caller work over 195 advances; the largest advance
was 72.799 ms. Entry publication took 347.836 ms over 71 advances, with a largest
advance of 45.205 ms. The synchronous comparison blocked for 1,110.413 ms during
construction and 514.184 ms during entry events. Source preparation added
177.090 ms in the staged fixture. These are back-to-back caller polls with capture
enabled, not presented live frames: they establish matching output and shorter
uninterrupted UI work, not lower total loading latency or a hard 8 ms ceiling.
Native construction and renderer admission can still exceed the loading allowance.
Those residual blocks are not covered by a hard frame-time guarantee.


### Joined M2 visible work

The ordered traversal now captures explicit visible inputs and transfers each
admitted model's particle and ribbon storage to a joined frame job. Workers own
simulation, particle/ribbon geometry, material packets and transparent sort keys.
They borrow immutable CPU resource-validation tables; Vulkan devices, queues,
allocators and recorders remain on their existing owner. No detached task or
per-frame clone of live particle pools is introduced.

Clock advancement, CRT consumption, completion/event callbacks, attachments,
light publication and shadow admission remain ordered. Existing unit palettes
are consumed once. A visible model with no CPU bone consumer can instead compose
its complete palette in the geometry job. Its CPU publication receives an
explicit empty transform view, so it does not run a second pose calculation.
Shadow upload offsets reserve the same ordered palette ranges. Models with CPU
bone consumers and shadow-only models retain the existing sampling path.

The join returns all placement-owned effect state, including on worker errors.
Publication merges output in traversal order with checked vertex/index and
producer-order relocation. The current admitted job count bounds scratch owners;
obsolete model generations are not pinned by the job list. Focused modules own
palette inputs, simulation/packets, ribbon updates and ordered publication.

A frozen pre-change traversal from `20b26515` is retained as an independent test
oracle. The comparison exercises 24 moving/faded models, visibility changes,
removal/compaction, particles, ribbons and shadows over 16 frames. It compares
complete packets, geometry, palettes, liquid ordering, simulation histories and
the shared CRT stream. Range-overflow and resource-failure cases exercise the
new ownership boundary. Formatting, all-target/all-feature workspace Clippy with warnings denied, and
all-feature workspace tests pass (1,412 passed, zero failed, 33 ignored). After
removing the redundant empty CPU pose traversal, Clippy passed again and the
complete runtime suite passed (466 passed, zero failed, 28 ignored).
Uncontended optimized and installed-world measurements are recorded below.

The first optimized geometry-only synthetic comparison (512 owners, 224 measured
frames) observed 4.480136 ms for the original traversal and 3.678358 ms for the
joined path. A later run overlapped workspace validation and is excluded from
performance conclusions. These are synthetic CPU preparation observations, not
matched Orgrimmar FPS or evidence of a five-millisecond total-frame reduction.

### Retained GameObject membership and changed publication

ActiveWorld retains GameObject membership in native admission order alongside
its existing unit membership. GameObject projection no longer discovers those
owners by probing every unrelated entity. Duplicate creates retain position;
remove/recreate receives a new lifetime at the end. World replacement owns a new
membership image.

Projection still resolves current parent placement, but an identical complete
instance image no longer reassigns resource/behavior handles or republishes the
owner. Passenger and map-model placement remain separate comparisons; errors are
compared in full rather than treating all invalid placements as equivalent.
Loading completion, native clocks and collision dependencies retain independent
owners. The implementation is separated into the coordinator's publication
module. F10 records changed publication separately from polled owners. The
Build 134 capture put this projection at about 0.064 ms on average, so this is an
ownership correction, not a claimed multi-millisecond saving.

### Repeatable hidden-world diagnostics

The installed-world benchmark has explicit `--hidden` and `--soap` options.
Hidden startup retains the native Vulkan surface without showing/focusing a
window. Soap selects the saved female blood-elf mage appearance/equipment fixture;
NPC populations remain explicitly authored and are not a recorded server crowd.
The benchmark records M2 packets, particle vertices and palette counts alongside
stationary/orbit/travel timings and advances F10 frame correlation. Hidden timing
does not establish visible-desktop FPS. It also uses the synchronous diagnostic
UI constructor and cannot establish live sliced-loading latency.


### Final autonomous measurements

The final optimized M2 comparison runs five times without concurrent compilers or
workspace tests. Each run compares 512 owners over 224 measured frames against
the frozen original traversal, with identical fixed simulation steps. Complete
output/state assertions execute outside the timed calls and pass in every run.

| Run | Original traversal (ms) | Joined preparation (ms) |
|---|---:|---:|
| 1 | 4.205884 | 3.382313 |
| 2 | 4.177312 | 3.334233 |
| 3 | 4.201904 | 3.346536 |
| 4 | 4.259024 | 3.364479 |
| 5 | 4.198076 | 3.327109 |
| Mean | 4.208440 | 3.350934 |

This is a 0.857506 ms (20.38%) reduction in this CPU preparation workload. It does
not establish a five-millisecond total-frame reduction or doubled live FPS.
Evidence: `target/m2-final-quiet-benchmark.log`.

Two hidden Vulkan city routes complete against installed stock assets, using
Soap's saved appearance, 2560x1440, Ultra shadows, four CPU workers, disabled
VSync, and 120 authored NPCs. The route starts at map 1, position
(1515.34, -4417.27, 18.0499), and includes 512 frames per phase. No desktop window
is shown or focused. The isolated diagnostic profile disables audio. This is an
offline scene workload; network sessions and live movement solving are absent.

| Phase | Capture off mean (ms) | Capture off p95 (ms) | Capture on mean (ms) |
|---|---:|---:|---:|
| Stationary | 5.269 | 5.856 | 5.228 |
| Camera orbit | 4.682 | 6.154 | 4.911 |
| Pointer | 5.817 | 6.199 | 5.887 |
| Travel out | 5.195 | 5.980 | 5.223 |
| Travel back | 5.331 | 6.071 | 5.363 |
| Settled | 6.167 | 6.732 | 6.230 |

Settled work is larger than the first stationary window: resident tiles increase
from 1.68 to 7.06 on average, particle vertices from about 5,007 to 12,386, and M2
packets from 753 to 778. The elapsed-time increase cannot by itself demonstrate a
leak or performance decay. Initial streaming still has an isolated 124.55 ms
maximum; the first buffer-growth frames are included. These routes do not prove
all streaming hitches resolved.

Capture `1789420438676-1` records zero dropped samples, event rows or capacity
overflows. Ordinary M2 preparation averages 2.4278 ms, joined geometry 0.4887 ms,
and ordered geometry publication 0.0950 ms. These scopes overlap and must not be
added. The routes use wall-clock animation, so capture-on/off differences are
observations, not a controlled estimate of instrumentation overhead. Neither
route can be compared directly with the older fixed-step replay or user-driven
Build 134 session to claim an FPS gain.

Evidence: `target/joined-world-final.csv`, `target/joined-world-profile.csv`, and
`target/joined-world-profile-profile/Profiles/capture-1789420438676-1.*`.

### Scope disposition

The identified mechanisms now have implementation and autonomous validation:
background saturation cannot hold the frame batch; static placement metadata
survives dynamic publication; independent M2 work runs in owned joined jobs;
GameObject membership and collision dependencies retain their owners; and world
entry/action dispatch no longer repeatedly discovers all regions and rebuilds
presentation after every event. GameObject parent transforms are still resolved
when polled because transport movement can change placement independently of an
object-field update. Unchanged publication is suppressed after that comparison.

The real 25,099-region FrameXML fixture validates the action/event consumer as
well as startup. The optimized indexed 144-action batch was approximately
18?20 ms total in the recorded runs; the debug before/after comparison above
isolates the former broad subscriber scan. This establishes removal of that
algorithmic source, not a replay of the exact live world-service spike. Frame
worker waiting, GPU fences and live service timings remain separately exposed
by F10; no wait is subtracted from total frame time to report a speedup.

Residual native startup blocks, first-use GPU admission, live network/audio
workloads and matched desktop FPS remain measurement limits. The implementation
work does not depend on another user-run test, and the numerical limits above
remain explicit rather than treating this as a claim that all performance work
or every possible hitch is finished.


The final capture also retains two long dispatch outliers. At frames 1269 and
1413, caller batch time is 34.885 and 31.273 ms, while dispatch alone is 25.729
and 30.833 ms. Caller cycle counts are only 82,606 and 49,816. The primary frame
worker's dispatch p99 histogram bound is 0.064 ms and mean 0.037 ms. This identifies
a wait rather than tens of milliseconds of caller pose computation. It does not
identify the external scheduling cause or prove that every long join is fixed.
The tested fix specifically prevents frame work from entering the background
archive queue; no priority override, busy-wait policy or speculative fallback is
introduced to hide these residual observations.


### Longer route and final Testing build 136

A third uncaptured hidden route runs 2,048 frames per phase (14,336 total) without
concurrent compilation. It completes successfully. The longer stationary window
continues admitting terrain: its first/last 512 frames average 4.00/9.76 resident
tiles, about 9,688/12,115 particle vertices, and 5.78/6.76 ms. Later pointer work
reaches 49 resident tiles. Travel reaches 56 tiles, then returns to 49.

The final settled phase holds exactly 49 tiles, 852 M2 packets and 11,016 palette
transforms throughout. Its mean is 6.866 ms and p95 7.363 ms. The first/last
512-frame means are 6.866/7.007 ms, while particle vertices decline from about
13,712 to 12,317. This bounds scene residency in this run; it does not prove the
absence of a slow leak or eliminate timing drift. The run still contains isolated
hitches (maximum 68.911 ms in settled work). Evidence:
`target/joined-world-long.csv` and `target/joined-world-long.log`.

The official package script reserved and built **Solarity 0.0.3a, Build 136**
from `bbb82e921c57ea1a71bdb6ae57d3bebd001c034d`. Installation and executable
identity verification passed on 2026-09-14. The dirty identity reflects package
reservation; all functional source changes were committed before compilation.
Installed and packaged SHA-256 hashes agree:
`767B46E71B1CDD9A041F88AE4D0EC8D6F3F1ABC71F60744913D548242C4452E4`.

The persistent Testing shortcut targets the installed build, retaining 2560x1440
fullscreen-windowed settings, four CPU workers, two network workers and manual
F10 activation. No visible client was launched. Required workspace checks and
final changed-runtime checks are recorded above. The identified implementation
work is complete and this numbered build is the delivery checkpoint; remaining
measurement limits and dispatch/streaming hitches are explicitly retained above.
