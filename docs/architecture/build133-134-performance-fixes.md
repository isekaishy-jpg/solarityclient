# Build 133–134 performance fixes

This work closes the measured issues from captures `1789368498983-1` and
`1789373560190-1`. It preserves stock event ordering, visibility, animation,
shadow consumers and collision results. Loading latency, moving-scene hitches
and sustained frame time require separate verification.

## Completion requirements

- [ ] World-entry UI: remove repeated broad presentation reconstruction between
  initialization events, preserve script-visible geometry, and address the
  synchronous 2.45-second initialization stall. Exercise real FrameXML loading.
- [ ] Frame-critical CPU batches: resolve the dependency behind the 75 ms pose
  join; isolate frame work from background loading without discarding required
  poses or introducing detached work. Validate under occupied loader workers.
- [ ] M2 preparation: establish explicit consumer requirements and retained
  preparation across animation, shadows, effects and packets; validate reduced
  work and unchanged output using representative city consumers.
- [x] Placement topology: ordinary dynamic membership changes must not rebuild
  immutable scenery metadata. Cover additions, retirement and source remapping.
- [ ] GameObject publication and collision: replace redundant polling/publication
  with owner changes and bound re-registration to affected geometry. Preserve
  moving transports, tile readiness and native candidate ordering.
- [ ] Validate the world-service action/event spike through the shared UI update
  changes; distinguish genuine work from fence/limiter/scheduling waits.
- [ ] Run required formatting, workspace Clippy and tests; record targeted
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
