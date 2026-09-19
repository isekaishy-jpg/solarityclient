# Retained static M2 publication

The Build 155 capture recorded 407 topology publications, but only 22 static
spatial-tree rebuilds. Cached static geometry facts survived; publication still
walked the full placement lineage and reconstructed parallel metadata arrays.
Topology spans reached 4.930 ms in that moving capture. Those live durations are
not the isolated cost or measured saving of this change.

## Structural journal and publication

Placement storage now retains its ordered dynamic indices and a sticky static
layout invalidation flag. Structural operations update both while they already
process membership. A new storage owner requires complete publication, and a new
visibility owner also requires one complete snapshot.

| Operation | Static publication requirement |
| --- | --- |
| Append/remove dynamics without relocating static indices | Retain static slots; publish dynamic state |
| Add, remove or replace a static placement | Complete publication |
| Compact/reorder a dynamic owner before surviving static records | Complete publication |
| Change dynamic appearance/ancestry at existing indices | Publish dynamic state |
| Rebase immutable source indices | Keep static facts; placement/source owners handle their own references |

Only complete publication acknowledges static invalidation. Publishing an effect
suffix cannot erase a pending change in the ordinary prefix. Effect ordering
finds the first effect through the dynamic index journal and stable-partitions
only that suffix; preceding scenery does not enter its permutation storage.

When static indices and identities survive, visibility retains their bounds,
distance categories, shadow facts, sorting flags, light flags and WMO membership.
It recomputes dynamic ancestry, first-owner lookups, retirement/game-object/effect
membership and callback traversal in native order. The required-work list removes
old dynamics and inserts current ones while retaining required static lights and
source-less owners. Static distance-class entries and spatial nodes are untouched.

Replicated-WMO keys and authored static-WMO keys occupy distinct owner domains.
Partial doodad publication updates only replicated keys, preserves first-occurrence
light ownership and restores numeric light order. It invalidates the previous
whole-membership comparison so a later return to that old full set still rebuilds
correctly. Unchanged replicated membership leaves the static lookup alone.

Static visibility facts no longer store a source-array index they never consumed.
Removing that field also removes the cache-wide remap during source compaction.
Actual placement and residency source references still perform their necessary
remapping. This does not change resource lifetime or source selection.

## Evidence and scope

The existing 26,000-scenery fixture checks all published flags, ancestry, light
membership and camera/distance/shadow work queries against a separate full scalar
oracle. It includes interleaved dynamic removal, static-index relocation, source
compaction before delayed publication, reused owner keys and empty/replaced scenes.
New journal, effect-partition, required-work and partial-WMO tests cover the new
invalidation decisions and restoration of older complete membership sets.

The optimized manual benchmark alternates the current full cached path and the
dynamic path for 40 pairs, timing the same outer topology publication in each.
Its baseline is not an older installed binary, and its result is not a live FPS
comparison. Validation and measurements are recorded in the
[cutover status](cpu-cutover-status.md).

F10's `m2.topology.rebuilt_placements` now counts entries whose fields were
republished, rather than reporting every resident on every topology change.
`m2.topology.retained_static_placements` reports retained static slots on dynamic
publication; `m2.topology.static_layout_rebuilt` reports complete publication.
`M2 dynamic placement metadata` contains the partial path's phase timings. The
outer `M2 placement topology` scope keeps its original full-operation meaning.
Counter totals across this boundary require the changed definition above.

Static index relocation still needs full metadata publication. Dense simulation
storage can still move records during structural compaction, and vector capacity
growth can copy retained metadata. Stable storage/identity work, complete domain
byte accounting, bulk work admission and the other CPU cutover requirements remain
open. This checkpoint does not assert a five-millisecond frame saving or completion
of the full cutover.
