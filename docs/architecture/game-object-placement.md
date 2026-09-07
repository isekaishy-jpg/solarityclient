# Replicated GameObject placement

GameObject movement now retains the packed local quaternion and non-living
passenger offset that the object-update decoder previously skipped. Transport
M2 and WMO presentation use one resolved matrix instead of reconstructing yaw
from the ordinary facing word. The same placement type is available to the
retained dynamic collision owner.

## Ownership and admission

`ObjectMovementUpdate` preserves `UPDATEFLAG_ROTATION` as an optional exact u64.
`UPDATEFLAG_POSITION` retains its GUID, local XYZ offset, and final orientation
word, including an explicitly zero GUID. These are independent of the living
MovementInfo transport snapshot.

`GameObjectMovement` owns the admitted packed quaternion and nonzero passenger
relationship in ECS. An omitted create quaternion becomes packed zero, as in
`0x004D3890` / `0x00714250`; its decoded rotation is identity even if the
ordinary movement facing word is nonzero. A repeated create of an existing
non-local GUID keeps the previous placement while refreshing sparse fields.
Removal and subsequent creation establish a fresh movement owner. Sparse field
projection does not reset movement.

Movement-only operation 1 carries living data and does not update this placement
component. Its separate native format and unit-only admission are documented in
[object movement updates](object-movement-updates.md).

`GameObjectPresentation` retains `GAMEOBJECT_FLAGS` and all four bytes of
`GAMEOBJECT_BYTES_1`: state, object type, art kit, and animation progress. The
complete dense update-field table remains authoritative for other fields,
including `GAMEOBJECT_PARENTROTATION` used by transport animation.

## Native math boundary

The implementation follows these build-12340 operations:

| Address | Operation |
| --- | --- |
| `0x00982340` | Signed X22/Y21/Z21 packed quaternion decoding |
| `0x004F45B0` | Local rotation plus passenger-parent rotation |
| `0x004F4320` | Ordered quaternion composition with final f32 stores |
| `0x004F4460` / `0x004C21B0` | Passenger offset through the parent's matrix |
| `0x004C1C40` | Quaternion matrix, including spilled f32 products |
| `0x004C1BF0` | Scale the local basis without scaling world translation |
| `0x0070CBE0` / `0x00712EE0` | Update and expose the GameObject's full matrix |

Packed W uses the positive hemisphere. When squared XYZ length differs from one
by less than 2^-20, stock returns W=0 without normalizing XYZ. Values outside
the valid sphere and that tolerance produce NaN W. The decoder preserves that
result; placement admission rejects invalid, non-positive, or singular matrices.

`GameObjectPlacementResolver` retains chain scratch and caches resolved matrices
by world/entity lifetime and exact input float bits. It follows GameObject
parents and detects missing objects and cycles on every resolution. Parent input
changes invalidate dependent placements; a removed/recreated GUID cannot reuse
the previous lifetime's matrix. Passenger position
uses the parent's scaled matrix, while orientation uses the native quaternion
composition and the resulting object keeps its own scale. Missing parents do
not silently become identity transforms. Parent categories requiring another
placement provider return `UnsupportedOwner`.

## Presentation

`RuntimeGameObjectPresentation` admits every visible GameObject in registry
admission order. Its world/entity identities distinguish removal/recreation and
world replacement even when a server reuses the GUID. Canonical model paths
share complete CPU preparation, including MDX/M2 aliases; each object retains
its current replicated inputs and resolved placement or placement error.

The bounded worker prepares one shared resource request at a time. The local
player's named transport takes priority and remains the loading-card resource
gate; ordinary objects reserve the executor's interactive lane. Retired-world
jobs are joined without publishing into a replacement world. Admission and
resource changes advance a scene revision; valid matrix changes update existing
placements without decoding or rebuilding the scene.

M2 and WMO renderer owners share uploaded sources across object lifetimes.
Missing parents hide existing placements while retaining their animation and
effect histories. Parent arrival restores those owners without restarting
neighbors. Removing and recreating an object establishes fresh playback.
Resource readiness remains separate from collision placement and eligibility.

Replicated WMO roots render their referenced default-set MODD attachments through
the shared M2 pipeline. Each root lifetime retains independent child playback
from CPU resource admission, even while its passenger parent is unresolved.
GPU admission borrows those timers and shares authored mesh/material sources.
Parent updates compose the resolved root matrix with each MODD local matrix in
place; they preserve child animation, particles, and ribbons. Display/resource
replacement retires the previous root's child owners. MODD BGRA tint and flags
follow the same path as terrain WMO attachments.

Both static and replicated WMO loaders select active doodads in first loaded
group MODR reference order (`0x007BF740`). Duplicate references retain one
root-local owner; unused MODD records do not load models or consume animation
randomness. Replicated roots currently select the default set. Group portal
visibility and interior-specific attachment lighting remain incomplete; these
attachments use the existing M2 frustum and environment-lighting path.

Generic M2 GameObjects now attach a shared behavior timer when CPU preparation
completes. Ordered packet handlers and scene callbacks mutate that same owner;
GPU placement consumes its scene sample. Missing placement therefore no longer
stops transition completion. The exact constructor coverage and remaining
specialized providers are described in [GameObject behavior](game-object-behavior.md).

Scene retirement releases instance histories and CPU source ownership. The
renderer still caches shared M2/WMO device buffers until renderer teardown;
bounded device-resource retirement across static, Glue, and dynamic owners
remains separate unfinished work.

`PlacedWorldModelDrawPlan::prepare_with_transform` accepts the same full matrix
used by M2 presentation. `set_transform` refreshes existing group and draw bounds
in place, retaining allocations and draw/group ownership. Identical matrices
skip this work. The prior transport path reconstructed this plan and its lookup
and bounds allocations every frame.

## Verification and remaining work

`game-object-rotation-native.txt` contains 540 unpack/matrix pairs captured by
executing the original `0x00982340` and `0x004C1C40` instructions with x87 control
word `0x037F`. The corpus includes signed components, near-unit tolerance cases,
and invalid combinations. `game-object-passenger-native.txt` adds 96 compositions
through the original quaternion, matrix, scaling, and point-transform routines.
Portable tests compare finite outputs bit for bit and check native NaN outcomes.
The fixture headers identify the executable hash. These captures do not replace
native functions or derive expected results from the Rust implementation.

Encrypted loopback tests cover packet decoding, duplicate creates, removal and
recreation, sparse field updates, and every truncation of a populated GameObject
create. Runtime resource tests exercise quaternion changes and missing-parent
arrival/removal through both synchronous and asynchronous synchronization.
Renderer tests verify that moved bounds change visibility and can be restored.
Vulkan tests upload real triangle meshes, check shared GPU handles, preserve
neighbor timers and random-stream position, and retire/recreate exact lifetimes.
They also cover ordinary models with only a Stand animation.
The WMO attachment Vulkan fixture captures distinct authored red/blue tints,
moves the parent and checks the new pixels, verifies an unresolved parent emits
no mesh/effect draws, and restores it without replacing timers or mesh handles.
It checks repeated MODR references and an unreferenced missing model, CPU timer
startup before GPU placement, random consumption, GUID reuse, and disconnect.

This implements replicated base placement, not animated transport trajectories.
GameObject type 11/15 animation/path clocks, other parent categories, destructible
owners and alternative WMO doodad sets still require their domain providers.
Ordinary visible GameObjects share presentation resources and retain dynamic
collision references. The production scene update synchronizes those references
after CPU model/behavior placement, and the combined movement collector reads
the same live behavior state.
The native dynamic collector calls `0x004F6560` through `DAT_00CE04B0`, tests the
object's collision eligibility (`0x0070F550` for GameObjects), uses its full matrix,
and can replace provenance with its transport GUID before `0x007A4B80` emits
faces. Admitted WMO roots and default-set doodads also participate in movement
collection and spatial registration. Specialized map-M2 owners, local
input/ground/fall application, and full-client FPS
remain unverified; see [movement geometry](movement-world-geometry.md).
