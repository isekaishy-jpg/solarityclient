# Critical frame-cost audit, September 12, 2026

Source baseline: `c276230a`, immediately after Testing Build 122 and the opt-in
frame instrumentation commit. This is a code audit, not a new performance run.
The review followed the live loop through session/ECS publication, population
residency, movement/camera queries, terrain and model publication, UI mutation,
audio, animation, Vulkan recording and submission. It also checked worker and
asset-loading boundaries reached from those paths. It is not a claim that every
parser, test or startup-only line in the workspace was individually reviewed.

The main problem is work admission and invalidation. Several public frame paths
can expand a small change into population-wide reconstruction, global dependency
discovery, synchronous asset preparation or driver pipeline creation. Making the inner
loops slightly faster cannot establish a predictable frame budget.

At 100 FPS a frame is about 10 ms; 300 FPS permits 3.33 ms and 1,200 FPS permits
0.833 ms. These budgets include necessary rendering and presentation. The last
1440p GPU replay commonly reported about 1.3–1.4 ms on the GTX 1070; that workload
already exceeds the stretch budget on the GPU alone. This does not explain the
populated live client's full 10 ms, or rule out much higher throughput at lower
resolution. These are elapsed GPU query intervals, including scheduling effects,
not an immutable hardware limit. CPU and GPU timings overlap and must not simply
be added.

## Priorities

| Priority | Problem | What is established |
| --- | --- | --- |
| 1, steady population cost | Reconstruct appearance before checking whether it changed; repeat population and animation walks | Repeated work is directly visible in the live call chain; live milliseconds remain unmeasured |
| 1, measured interactive cost | Discover a tiny UI dependency island by repeatedly scanning the full arena | Existing trace measured 1.694 ms of discovery for 29 affected regions among 25,295 objects; discovery remains in current code |
| 1, admission stalls | Create cold ordinary and shadow driver pipelines during GPU publication | A cache miss synchronously creates Vulkan pipelines on the presentation owner; the original shaderc attribution was incorrect (see section 3) |
| 1, admission stalls | Admit complete remote appearances and changed terrain generations in one frame | Synchronous decode/composition is reachable; recent terrain evidence still has roughly 11.47 ms average changed-frame streaming |
| 2, population-dependent candidate | Repeated all-root registration/liquid queries and two camera solves | Multiplicative population/root traversal and duplicate solve entry points exist; contribution needs isolation |
| 2, population-dependent candidate | Recompose and republish complete bone palettes/material streams | Every admitted pose is sampled and serialized; immutable hierarchy facts are rediscovered during sampling |
| 2, interaction and burst stalls | General UI publication and network draining have no small-work guarantee | Full UI rebuild remains reachable; a bounded packet queue is drained without a per-frame service limit |

## 1. Appearance reconstruction is part of an unchanged frame

[`synchronize_remote_players`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/player_coordinator.rs#L2003)
calls `resolve_desired_remote_player` for every remote player before its equality
test. That resolver builds equipment vectors, an attachment plan, a base texture
plan and a geoset plan, and canonicalizes the model path. Those are appearance
inputs, yet movement-only and idle frames reconstruct them. The creature path
likewise resolves models, body scale, item definitions and weapon state before
testing its newly constructed desired list against residents.

[`visible_unit_guids`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/ecs/src/world/state.rs#L319) walks the entire
object registry, acquires component access to classify each entry, allocates a
vector and sorts it. A normal active-world frame reaches it through creature
residency, remote-player residency, remote movement, and passenger synchronization
both before movement and after remote movement: five independently materialized
population lists. This is replicated visibility, not camera visibility.

Both residency functions also call
[`synchronize_replicated_animations`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/player_coordinator.rs#L1490),
which traverses creatures **and** remote players, including binding and opacity
synchronization. The two passes occur at different publication points, so removing
one requires preserving creation/retirement ordering, not merely deleting a call.

Required contract: lifetime/appearance changes produce a retained change set;
motion and animation consume retained owners independently. Keep deterministic
order and exact object generations. Movement, vehicle ancestry, equipment and
opacity changes must remain visible without reconstructing unchanged appearance.
This is the first steady-frame target for the populated-world gap. No specific
millisecond saving is claimed before comparing the same population.

## 2. Local UI changes still have global discovery cost

[`refresh_dependency_regions`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/ui/src/region/geometry.rs#L242)
allocates an arena-sized affected mask and scans every object and its anchor
edges until no new dependents are found. With backward-index dependency chains,
multiple full scans are required. Discovery is O(passes × (objects + anchors)),
even if the eventual changed island is tiny.

The [latest geometry investigation](world-performance.md#borrow-retained-geometry-during-tooltip-layout)
measured 1.694 ms in discovery while actual resolution of 29 regions took 0.009 ms.
The subsequent borrowed-seed change removed other work; it explicitly left this
discovery algorithm intact. That historical measurement supports the priority,
but is not a fresh timing of the final build.

Required contract: maintain reverse parent/anchor edges when topology or anchors
change, and walk only reachable dependents. Preserve retargeting, transitive
inheritance, cycle errors and transactional publication. This addresses an
already measured millisecond cost rather than optimizing individual arithmetic.

## 3. Cold driver pipeline creation can run synchronously

Correction during implementation: `M2SpirvCompiler::compile` selects build-generated
SPIR-V and specialization values. It does **not** invoke shaderc at runtime. The
original version of this audit incorrectly attributed source compilation to this
call. Both ordinary and primary-shadow bytecode are already available.

`M2PipelineRegistry::prepare_precompiled` still creates the ordinary Vulkan
pipeline and its primary-shadow partner on a cache miss. Source publication can
request several material/fade variants, and static scene publication can encounter
several cold sources in one call. This is a driver-work admission problem.

Required contract: explicit cold-program admission and shared immutable source
plans before visible publication. A mesh program includes its shadow partner;
one admitted program is not a hard time bound on an individual driver call.

## 4. Whole-generation admission still crosses the frame boundary

[`load_remote_player`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/player_coordinator.rs#L2182)
loads the model, resolves equipped texture layers, composes the body atlas and
loads optional textures synchronously. Its caller loops over all changed remote
players. Creature cache misses similarly load resources inside their synchronous
residency pass. The following replacement publishes their GPU resources in the
same frame service. This bypasses the worker/admission model already present for
local-character and game-object preparation.

[`TerrainFrame::synchronize_tiles`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/terrain_frame/streaming.rs#L47)
prepares every arriving tile's terrain/liquids, then all new M2 and WMO membership,
before returning. It is called from
[`service_terrain_streaming`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/client_services/world_camera.rs#L63).
Completed CPU jobs do not bound the ensuing main-thread publication transaction.
The recent tooltip-materialization replay in [world-performance.md](world-performance.md)
still reports 11.466283 ms mean streaming over 96 changed frames and up to
12.5382 ms streaming in the cited run. Earlier 20–30 ms figures are historical;
they must not be presented as the latest cost after intervening fixes.

There is another first-visibility admission path in
[`GroundDetailWorld::prepare`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/terrain_frame/ground_detail.rs#L74):
every newly visible uncached chunk scatters detail, builds a complete mesh and
requests texture upload inline. Multiple chunks can enter this path in one
camera turn. Its cached steady state is different from its cold reveal cost;
this is another publication candidate, without a measured millisecond attribution
from this audit.

Required contract: worker-prepared immutable generations, an explicit bounded
publication queue, and atomic activation after preparation. Scheduling must keep
CPU collision and GPU terrain generations consistent. Merely postponing the GPU
half after committing CPU terrain would reintroduce the publication bug covered
by `terrain_publication_precedes_recoverable_unit_errors`.

## 5. Spatial work scales with units times resident roots

[`RuntimeRemoteMovement::service`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/player_movement/remote/service.rs#L69)
queries submerged liquid for every eligible remote owner after advancing it.
[`unit_submerged_liquid`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/terrain_coordinator/movement/liquid.rs#L120)
performs unit registration; its
[`probe_registration_roots`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/terrain_coordinator/movement/registration.rs#L191)
visits all resident WMO roots. General liquid selection can visit all roots again.
Individual roots reject irrelevant bounds, but the outer traversal still scales
with the unit count times the root count, including stationary owners.

[`resolved_world_camera`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/client_services/world_camera.rs#L11)
is called during terrain streaming and again during presentation. Each call runs
obstruction/water/volume queries and publishes feedback; there is no retained
final-frame camera result at this boundary. These are distinct times and may
observe changed terrain, so unconditional reuse would be incorrect.

Candidate contract: ordered spatial candidate indexes and retained registration
keyed by position, bounds and geometry/parent generations; one coherent camera
solve per applicable generation. Preserve stock root precedence and feedback
ordering. These are plausible large costs in dense scenes; no measured saving is
assigned, and the existing movement sweep cache must not be described as absent.

## 6. Animation rebuilds derived structure as well as changing values

The M2 traversal calls
[`recompose_with_overrides`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/terrain_frame/m2.rs#L2903)
for each admitted model. The
[`pose composer`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/rendering/src/model/m2_animation/pose.rs#L243)
samples all bone tracks and composes the entire hierarchy. Sequence selection
walks ancestry and searches override roots per bone; finger classification walks
the immutable parent tree again when its overlay is active. The frame owns one
scratch pose, not retained results for each independent instance.

The resulting palettes enter a frame-wide vector, and
[`WorldFrameSlot::write`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/rendering/src/device/vulkan_world_frame/resource.rs#L421)
serializes every matrix, material record, particle vertex/index and ribbon vertex
again. Per-instance animation and transparent ordering are necessary, but
unchanged bind poses, immutable hierarchy classifications and unchanged material
fields have no equivalent retained publication contract here.

Candidate contract: precompute immutable ancestry/classification and bind-pose
facts; retain poses only where all animation, override and camera dependencies
permit reuse; separate immutable material storage from dynamic frame fields.
Preserve per-owner random consumption, events, billboards, attachments and hidden
light/effect clocks. This is a throughput candidate, not permission to freeze
necessary animation or a claim that every pose can be shared.

## 7. Other paths can admit an entire backlog or UI rebuild

The general
[`refresh_targeted_objects`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/ui/src/glue/c_glue_mgr.rs#L1023)
path, after retained-path eligibility fails, resolves all geometry, presentation,
mesh and pointer state. New object topology reaches `refresh_live_state` as well.
The recent tooltip fixes narrow ordinary cases; they do not guarantee that one
new widget, glyph or mixed layout transaction remains local. Packet ownership and
dependency indexing are the general solution; more widget-specific shortcuts
would leave the same failure mode reachable.

[`RuntimeGameplayCoordinator::service_with_game_objects`](https://github.com/isekaishy-jpg/solarityclient/blob/c276230a/crates/runtime/src/application/gameplay_coordinator.rs#L440)
drains `try_recv()` until empty or a lifecycle boundary. The channel capacity is
256, but it can refill during the drain, and one packet can contain many object
updates. There is no per-frame work admission limit. Treat this as a burst/stall
risk; introducing batching requires preserving wire order and transactional
object updates, not dropping packets or inventing stock recovery behavior.

## Findings deliberately not promoted to critical fixes

- Terrain already rejects whole tiles before testing chunks. Point lights already
  use a spatial grid. Treating either as an unconditional world-wide brute-force
  query would misrepresent the current implementation.
- M2 geometry and major texture uploads already retain deferred transfers.
  Frame-slot fence waits enforce resource ownership. Device-idle calls inspected
  here serve surface/slot changes, shadow image identity/quality transitions,
  explicit capture or teardown; they are not proven steady-frame idle waits.
- Audio settings and listener updates already compare retained values before
  reapplying the mixer work. CVar parsing and small bookkeeping alone have no
  demonstrated millisecond cost warranting a critical fix in this slice.
- Bounded workers are present for terrain, local-character and game-object
  preparation. The critical gap is which live paths bypass them and how completed
  work is admitted, not the mere absence of a worker pool.

Next implementation order: population appearance/change ownership and UI reverse
dependencies for recurring work; complete worker resource preparation and bounded
appearance/terrain publication for stalls. Use the existing live scopes to check
the same populated scene before/after each change. Spatial and pose work follows
when its scope is large enough to justify the next structural change. No behavior
or fallback policy was changed by this audit.

Implementation and remaining cost boundaries are recorded in
[critical-performance-implementation.md](critical-performance-implementation.md).
Source links above are pinned to the audit baseline; the implementation has
since decomposed several of those files.
