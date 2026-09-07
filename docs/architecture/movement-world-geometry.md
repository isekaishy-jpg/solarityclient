# Movement world geometry

`solarity-systems::collision::movement_collection` supplies movement triangles
from admitted ADT, WMO, and M2 generations. The existing camera trace remains a
separate consumer. Movement requires its own triangle order and normals because
the narrow phase resolves equal contacts in provider order.

## Native evidence

The pinned build-12340 executable has SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

| Address | Responsibility |
| --- | --- |
| `0x007A5F20` | WMO owners, global square conversion, ordered world chunk visits |
| `0x007A55E0` | WMO residency/placement traversal and local query-box transform |
| `0x007A5A60` | Resident ADT/MCNK lookup, terrain, then chunk object references |
| `0x007D8840` | Hole-filtered terrain squares and movement fan order |
| `0x007A61D0` | Terrain vertex outcodes with tolerance at `0x00A3FDB8` |
| `0x007AEF00`, `0x007AE140` | Ordered WMO groups and ordinary movement mask |
| `0x007CB7B0`, `0x007CA920` | Positive-child-first BSP traversal |
| `0x007CA8C0`, `0x007C9B10`, `0x007C7A00` | Leaf faces, visited flags, triangle box rejection |
| `0x007C7230`, `0x0079B1F0`, `0x0079AE80` | Enabled BSP cache, cached outcodes, cache admission |
| `0x00782740` | WMO face transformation and calculated normals |
| `0x0082EC30` | M2 vertex transformation and authored collision normals |
| `0x004C21B0`, `0x007F93D0`, `0x007F9320` | Matrix-point and recentered box arithmetic |
| `0x007912C0`, `0x0079E7C0` | Scalar plane construction and CPU-dependent SIMD selection |

`MovementCollisionBounds::terrain_chunks` returns ADT/MCNK addresses in native
world-row then world-column order, including across tile edges. The world axes
transpose when converted into ADT filename coordinates. Conversion preserves
the native float stores before FISTP: the first Y-min subtraction remains wide,
while the other three reload stored floats. At a chunk boundary, treating those
four expressions identically can add triangles stock does not select.

`TerrainCollisionMesh::append_movement_chunk` consumes one such chunk address.
It applies authored 2-by-2-square hole bits and the four native fan offsets:
`[17,9,0]`, `[9,1,0]`, `[9,17,18]`, `[9,18,1]`. Camera fan order is different.
Outcodes use local MCNK vertices and the native approximately 0.01944444-unit
tolerance; emitted vertices are world-space floats.

`PlacedWorldModelCollision::append_movement` checks root/group bounds, visits
the positive BSP child first, and retains each admitted face's first visit.
Group selection uses root MOGI bounds, while BSP traversal regions use each
group's MOGP bounds. The asset owner retains both independently.
Ordinary movement excludes MOPY `0x04`, while `0x02` remains camera-specific.
The transient `0x80` visited state is represented by reusable separate storage.
Only leaf-node face ranges are submitted. Calculated normals transform the
retained local cross product before normalization; they are not reconstructed
from already-rounded world vertices. Native degenerate WMO normals become +Z.

`MovementBspCacheMode` preserves the resolved `bspcache` setting. Registration at
`0x0078E400` supplies the string `"1"` at `0x009E1464`; caching is enabled by
default. `0x0078DF90` owns changes to the setting. Cached leaves reject faces
wholly on a box boundary, while uncached leaves use strict outside tests.
Native cache admission requires at most 300 face references and 450 distinct
vertices. Larger leaves fall back to the uncached path even when caching is
enabled. Eligibility is prepared with the immutable generation, so queries do
not rebuild the native cache's derived vertex table or need its eviction state.

`PlacedM2Collision::append_movement` retains authored face order, including
finite degenerate faces. It uses transformed world-vertex outcodes and the
dedicated face-normal array. Each placement axis is normalized independently
above the squared-length threshold at `0x009EA27C`; the authored normal is then
transformed without renormalization. Render mesh vertices are never substituted.

## Ownership and arithmetic

Callers retain their output vector. Append errors invalidate the collection;
the caller must discard partial results. WMO traversal scratch is retained by
the placement, preventing repeated traversal allocations once its largest group
has been visited. Terrain and M2 selection allocate only when output grows.

`MovementCollisionTriangle::with_normal` preserves the provider's finite float
normal, including M2's non-unit and zero normals. The existing `new` constructor
still derives a unit normal and rejects degenerate input geometry.

`PlacedWorldModelCollision::prepare_transforms` admits the independently stored
forward and inverse matrix images resolved by a placement/transport owner.
Re-inverting one rounded matrix can put a point on the other side of a BSP
boundary. The convenience constructor deriving an inverse is available for
ordinary geometry callers; it is not evidence for stock placement-state updates.

Stock enables an unrefined `RSQRTSS` estimate when SSE is available. The CPU crate
provides that instruction through a safe, feature-tested boundary; unsupported
architectures use the evidenced scalar reciprocal-square-root path. Cross
products and point transforms preserve x87 store boundaries with wider
intermediates. This is not a universal promise of bitwise x87 equivalence.

## Independent validation

`movement-collection-native.txt` contains 2,247 queries of the original x86
instructions: 350 terrain, 1,267 WMO, and 630 M2 cases. Terrain and WMO cases run
the complete `0x007A5F20` entry with explicit resident native structures. M2 cases
run `0x0082EC30` with resident dedicated arrays and preallocated output. No
collection function is replaced. A read-only hook records terrain chunk visits.

The fixture supplies two ADT coordinate ranges, asymmetric chunk origins and
heights, authored holes, zero-width and boundary boxes, and deterministic random
boxes. Mesh cases exercise translations, rotation, uniform and nonuniform
matrices, non-unit authored normals, filtered WMO faces, and duplicate BSP face
references. The WMO fixture supplies both matrices as independent native inputs.
Every ordinary WMO case executes with caching enabled and disabled. Four
additional cases cross both cache admission limits at a coplanar query boundary;
the real native cache lookup/population functions execute in enabled captures.
Three cases distinguish MOGI group-selection bounds from MOGP traversal bounds.

Tests load corresponding synthetic assets through the real archive/asset APIs.
They assert exact chunk visitation order/count, candidate count/order, vertex
coordinates within 0.00001 units, and normal direction within 0.000001.
Unicorn computes RSQRTSS as an exact reciprocal square root; real hardware uses
an estimate. Normal magnitude comparison therefore permits the instruction's
relative error bound. Separate CPU tests exercise the public instruction
boundary. Invalid world queries cannot become successful empty collections.

## Resident static queries

`RuntimeTerrainCoordinator::collect_static_movement` joins these collectors to
the [resident ADT neighborhood](terrain-streaming.md) and global-WMO maps.
`RuntimeStaticMovementQuery` retains candidate, owner, and first-visit storage.
Each selected triangle maps to its authored MCNK, MDDF, MODF, or MODD owner.
These identities are scoped to the query map; they are not network GUIDs.
Native static triangle object identifiers are zero. Dynamic object identifiers
are retained by the combined query described below.

An unavailable matching map returns `PendingMap`. A query overlapping an
unavailable WDT-declared ADT returns `PendingTile`; an undeclared WDT tile is
empty space. Pending and failed queries clear all output. Global-WMO maps bypass
the terrain grid, as `0x007A5F20` does. A successful **static** result is not yet
authorization to commit player movement against an incomplete dynamic scene.

WMO registrations survive primary-tile promotion and are retired only after
their last resident reference disappears. `0x007BF1A5` appends roots through
`0x006DED60`. `0x007C6150` registers terrain placements in MCRF order, and
`0x007BF740` registers active MODD records in group MODR order. The runtime
prepares these references on the terrain worker and resolves them without
rescanning placement tables during each query. Roots precede terrain chunks;
each root's faces precede its groups' M2 references. M2 first visits are shared
across groups, chunks, and ADTs, matching the stamp at `0x007A50C0`.
The retained dedicated M2 box (`0x007BDB10`, header +0xBC) rejects distant
placements before the face collector, so a selected group does not require
transforming every referenced model's collision mesh on each query.

Placed group-reference bounds come from root MOGI (`0x007BDE50` / `0x007AE720`),
independently of MOGP BSP regions. The asset decoder reads nested MODR/MOLR
arrays directly from their validated chunk extents because `wow-wmo` 0.7's
nested parser omits them. Duplicate references and file order are retained;
incomplete u16 values and references outside the root tables are rejected.

`runtime/tests/fixtures/movement-residency-native.txt` records the original
`0x007A5F20` instructions over three root registrations, two groups per root,
repeated group M2 references, and shared MCRF references in two ADTs. The two
successful calls emit the same 11 owner tags; removing a declared ADT returns
false with partial candidates, which the runtime must discard. Diagnostic
nonzero object IDs label the native providers; enabled high M2 mask bits admit
those labels. No collection function was replaced. Runtime integration tests
compare the resulting order to this capture and cover repeated queries,
promotion, WDT holes, unavailable neighbors, wrong maps, invalid grid bounds,
and global-WMO queries. Separate asset tests cover nested references after MLIQ.

A local installed-client probe loaded Stormwind ADT `[30,48]` (6,210 M2 owners,
one WMO) and Northrend `[21,30]` (822 M2 owners). Narrow horizontal queries
with a tall vertical range selected 12 terrain / 6 WMO faces in Stormwind and
8 terrain / 8 MDDF faces in Northrend. Each query repeated 1,000 times with
identical vertices and normals. This is CPU geometry validation, not live input,
stock placement-matrix parity, visual parity, or a full-client FPS measurement.
Wider queries also selected 12,145 WMO and 8,159 MODD faces in Stormwind,
and 3,126 MDDF faces in Northrend, exercising the recovered group references.

## Retained replicated geometry

`ClientServices` calls `synchronize_game_object_movement` after current CPU model
and behavior synchronization. Generic M2 owners reuse the collision instance
already retained by their behavior. The spatial registry keeps model generation,
display, placement, and world/entity lifetime, plus per-group/per-chunk dynamic
lists. Changed placements remove their previous references and insert at the
head of each new destination (`0x007C2F80`, `0x007B5020`). Unchanged frames retain
list order and allocations. Static generation changes recheck destinations;
unchanged destinations keep their relative order. `0x007C1660` treats an
unavailable ADT as no registration floor, and `0x007C2040` links the available
chunks while skipping missing/loading ADTs. An object can therefore have an
empty or partially resident destination list without blocking unrelated
movement. Terrain publication invalidates these lists for re-registration.
`0x007A5A60` still rejects an unavailable declared tile touched by the movement
query itself. Build 27 incorrectly promoted a distant object's missing
destination to a map-wide gate, freezing both walking and falling; the runtime
regression exercises both responses beside a distant unloaded object.

`collect_movement` uses the same traversal as the static query. Each group's or
chunk's static M2 list is followed immediately by its dynamic list. Native
`0x007A5240` requires mask `0xF00000` and stamps an owner even when its callback
or box rejects it. The runtime stamps exact lifetimes independently of reported
GUIDs, resolves the current GameObject, and reads its retained behavior flag and
door query bit `0x8000`. `0x004F6560` reports a nonzero passenger-parent GUID in
place of the object's own GUID. Multiple children sharing that reported GUID
still contribute separately. Family masks also gate static M2 (`0xF`), WMO
faces (`0xF0`), and terrain (`0x100`); face material selection remains ordinary
movement, rather than a general-purpose native ray-query API.

Admitted replicated WMO roots share append order with MODF roots, matching
`0x007BF120` / `0x00783500`. Their faces carry the replicated lifetime and GUID.
The resource worker also completes default-set MODD sources. Those attached
collision instances follow group MODR order and carry native GUID zero.
Map-M2 construction at `0x007C21E0` initializes both GUID words to zero;
MODD construction and root-motion updates do not replace them.
Root motion updates placed bounds and attached doodads while retaining local
BSP caches (`0x007B67B0`, `0x007B40F0`, `0x007C1380`). Registration includes
these roots in the transformed bank, so a generic prop can attach to a moving
WMO group. `RuntimeMovementRegistrationQuery` reports the selected authored or
replicated root through `RuntimeWorldModelMovementOwner`.

Portable runtime tests cover multi-chunk deduplication, stable list order,
matrix and display replacement, missing destinations, parent GUIDs, door and
family masks, GUID reuse, world replacement, root append order, repeated MODR
references, and a prop registered inside a moving WMO. Separate placement
updates compare retained cached/uncached geometry to fresh transformed instances
and verify that invalid matrix updates preserve the last valid placement.

## Interval collection bounds

`MovementIntervalRequest::collection_bounds` implements `0x0075FF90`'s body
and candidate boxes in the already-resolved collection coordinate space.
Ground bounds include the forward support reach, radial step region, raised
step trial, and downward support probe. These probes remain present at zero
requested distance. Airborne bounds use the absolute normal/slow fall curve at
the wrapping end clock, retain the launch-height offset, and expand horizontally
for descending contact correction. Swimming/flying bounds have their separate
radial/vertical branch; this does not implement those modes' collision response.
All branches add the original contact tolerance after their native float stores.

The ground radial calculation retains the X center in x87 while reloading Y/Z
from floats. The fall curve stays wide through height subtraction and candidate
translation. Premature float conversion would change query boundaries.

`collect_movement_interval` supplies the expanded box to the retained runtime
collector. `RuntimeMovementQuery::interval_bounds` publishes body/candidate
bounds only with a complete result. Invalid inputs, pending residency, and
failed geometry clear the bounds, candidates, and provenance together.

`movement-interval-bounds-native.txt` contains 1,408 captures from original
`0x0075FF90` entry through `0x00760515`, immediately before world-query mask
selection. No native callee is replaced. Tests compare all body and query float
bits, including both unit step profiles, stationary travel, falling modes,
large/wrapping clocks, and translated coordinates. Runtime MPQ fixtures also
show a body-only query missing a lower floor, the expanded query supplying it
to a successful fall landing, and a private step region reaching an unloaded
neighbor and invalidating the whole result.

These captures use no passenger parent. Transport conversion, native query-mask
selection, and relaxed residency policy for mask `0x80000000` still require
their outer movement providers. The ordinary collector requires every declared
generation intersecting the expanded region to be resident.
The initial region does not replace native per-sweep cache-miss recollection.

## Per-sweep cache refresh

`MovementCollisionVolume::sweep_refresh_bounds` implements the world-space
`0x0075F0A0` fallback. It tests both translated body corners against the previous
box inclusively. A miss pads the endpoint body by native float `0x3E2AAAAB`
(approximately 1/6 unit), then joins that box to the entire previous region.
It neither shrinks the old region nor expands only along the traveled axis.
The shared narrow-phase extrusion helper preserves `0x0075F9D0`'s minimum
extrusion and bypass below `2^-20` travel.

`RuntimeMovementGeometry` borrows one terrain/world/object context with fixed
query flags and cache policy. `collect_interval` establishes initial coverage;
`prepare_sweep` reuses candidates on hits and recollects the complete union on
misses. Coverage is published only after success. Pending or failed queries
invalidate coverage, candidates, and provenance and require a new interval
collection. Creating a new context clears previous output, so a ready region
cannot cross world/object/flag changes. Selected owners can be copied before
subsequent probes reorder or invalidate the candidate array.

`movement-sweep-cache-native.txt` records 1,576 decisions and query float images
from original `0x0075F9D0` and `0x0075F0A0` execution. The capture passes no
origin override (which would bypass refresh) and no transport parent. It stops
before query-mask resolution on a miss, or at the original return on a hit;
no instruction or callee is replaced. Tests compare every decision and bound
bit. Runtime fixtures verify discovery of a previously uncollected floor,
retention of the old region across successive misses, hit reuse, and complete
invalidation when a sweep reaches an unloaded declared tile or has invalid input.

Ground and fall `advance_with_geometry` now consume the scoped adapter through
`MovementGeometry`, including every step probe and speculative fall. Legacy
`advance` wraps its fixed slice in an always-ready provider. Runtime contact
results contain copied `RuntimeMovementOwner` values, so later recollection
cannot change their meaning. The adapter retains the first unavailable probe's
pending/error cause until a new interval collection; subsequent probes in that
invalidated context remain unavailable.

The solvers preserve native partial state on unavailable probes. Ground exits
retain earlier position/step writes without final reanchoring or contact
notification. Fall exits retain prior movement/contact and report full consumed
time, but exclude the unavailable portion from the motion/fall clocks. Private
fall trials restore their own motion/fall clocks while propagating the native
controlled-subject skipped-time notification and heartbeat postponement.
These result fields still require application
by the outer movement owner.

Runtime fixtures now advance ground and fall directly through the provider,
including a first-sweep refresh that discovers a lower floor and a sweep into
an unavailable neighbor that preserves the original fall state and pending
tile cause. This establishes response/provider integration, not live keyboard
movement, transport conversion, or arbitrary world movement parity.

## Runtime work remaining

Residency still admits whole ADTs atomically. New roots use first MCRF reference
order across that generation's row-major chunks; exact registration timing
under native per-MCNK load priorities is not yet reproduced. Existing roots
retain their order across admission and promotion.

Replicated GameObject quaternion and passenger matrices now have a shared
native-verified placement provider used by transport M2/WMO presentation; see
[`game-object-placement.md`](game-object-placement.md). Those current matrices
now drive admitted collision owners; animated transport paths remain separate.

The owner also needs authored static placement-matrix verification, specialized map-M2 GameObject owners,
destructible WMO states and alternative doodad sets, and liquid/WDL modes.
The [initial local movement owner](local-player-movement.md) now connects
timestamped input and ground/fall application to ECS state. None of the
collector tests establishes live player movement or frame-rate parity.
