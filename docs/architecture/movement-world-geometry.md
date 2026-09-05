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
and transport ownership still belong to the movement interval's other providers.

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

## Runtime work remaining

Residency still admits whole ADTs atomically. New roots use first MCRF reference
order across that generation's row-major chunks; exact registration timing
under native per-MCNK load priorities is not yet reproduced. Existing roots
retain their order across admission and promotion.

Replicated GameObject quaternion and passenger matrices now have a shared
native-verified placement provider used by transport M2/WMO presentation; see
[`game-object-placement.md`](game-object-placement.md). It still needs to be
joined to dynamic collision residency and animated transport path owners.

The owner also needs authored static placement-matrix verification, collision-query
cache bounds covering step trials, dynamic-object admission, liquid/WDL modes,
timestamped input, and landing/ground/fall application to ECS state. None of the
collector tests establishes live player movement or frame-rate parity.
