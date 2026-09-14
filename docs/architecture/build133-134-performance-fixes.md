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
- [ ] Placement topology: ordinary dynamic membership changes must not rebuild
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
Workspace Clippy passes with warnings denied after these changes. The earlier
complete workspace run passed before the event index and final sound-scope edit;
the final numbered build still requires complete workspace verification.

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
