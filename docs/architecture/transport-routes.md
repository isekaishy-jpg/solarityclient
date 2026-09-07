# MO transport routes

`solarity-systems` retains the original type-15 route geometry, arrival/event
clocks, animation phases, wave physics, and per-object stop/resume state. The
specification is the pinned `Wow.exe` with SHA256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

## Construction and placement

`TransportCatalog` loads the exact TaxiPathNode (11 words), TransportPhysics
(11 words), TransportAnimation (7 words), and TransportRotation (7 words) schemas
through normal archive precedence. Route and animation owner lookups preserve
the signed-key lower-bound behavior of `0x007F7AD0`, `0x0070C8C0`, and
`0x0070C930`. Nodes within an owner remain in stored order; the loader never
sorts controls by their index or strips endpoint controls. Missing owner ranges
are empty; missing physics keys remain absent. The installed stock tables have
22,586 route controls, 5,262 position keys, 227 rotation keys, and three physics
records. All three owner-indexed tables satisfy the native signed ordering.

`0x007F92F0` initializes the route from the template's path, speed, acceleration,
and optional TransportPhysics record. `0x007F90F0` consumes TaxiPathNode in stored
order. A map change or the preceding node's flag one ends a continuous section.
Flag two adds a stop except at the section's first control. Endpoint controls
remain intact: each spline traverses from its second to its penultimate point.

`0x007F8660` builds sections through Path.cpp (`0x004C3830`). The distance to a
control node (`0x004C3720`) sums completed segment lengths in x87 precision.
The first leg decelerates toward a stop; internal legs accelerate and decelerate;
the last accelerates away. Sections with no stops use constant speed. Native
float stores before FISTP determine integer millisecond arrivals, with a separate
float-distance argument at `0x007F7A60`. Arrival/departure events use their own
distance calculation and include dwell time only at the exact stop node.

`0x007F7FC0` replaces the total period and final section end with GAMEOBJECT_LEVEL.
It does not rescale the stop/event table. `0x007F82B0` selects a section, applies
the stop window or leg profile, samples the retained spline, and takes yaw from
the negative horizontal tangent. The model sequence is Stand, ShipStart,
ShipMoving, or ShipStop (0, 162, 163, 164). Sampling allocates no memory.

An absent route has no placement. Invalid geometry, nonpositive or nonfinite
motion properties, a stop on the final control, and unrepresentable signed
millisecond leg durations fail admission. Native assumes well-formed input at
these points; Rust rejects it instead of reading uninitialized storage, producing
nonfinite world transforms, or silently saturating an integer conversion.

## Physics and stopping

`0x007F7DD0` attenuates slow movement before computing banking from four historical
raw path derivatives (`0x004C3920`/`0x004C42C0`). `0x007F7B30` adds vertical bobbing,
roll, and pitch from the ten TransportPhysics floats. Its wave periods use
**truncation**, established by FLDCW with rounding bits 0xC00 before FISTP; the
decompiler's generic ROUND notation alone is insufficient. Channel phase offsets
are 0, 0.5, and 0.2 radians. A missing physics row produces no added motion.

`TransportRouteClock` owns the per-instance state independently of route geometry.
`0x007F8120` anchors replicated u16 progress. `0x007F8000` requests a station stop
or resumes from a frozen departure. `0x007F80A0` freezes immediately at the station
selected from clock minus one. `0x007F7840` detects the departure within the frame
interval using `0x007F77D0`, including a wrapped interval. All native clock sums
retain unsigned 32-bit wrapping.

## Verification and integration

`tools/ghidra/transport_route_oracle.py` executes the original constructor,
finalizer, spline math, route/physics sampler, and stop clock. It intercepts only
the original allocation/free/reallocation ABI, including its no-in-place-copy
flag. Fixtures contain synthetic DBC rows, not redistributed game data.

The test compares 28 constructed routes, every generated event, 6,588 placements
and animation phases, and 11,200 stateful clock samples. Cases cover curved and
long paths, multiple stops, zero-distance first stops, map transitions, same-map
teleports, server period changes, uint32 rollover, repeated stop/resume requests,
replicated phase endpoints, missing physics, and disabled wave channels.

`game_object_transport_pose` reproduces the Z/Y/X Euler matrix writes in
`0x007134A0`, including the float stores in `0x004C3380`, `0x004C3340`, and
`0x004C3300`. The separate `0x00982910` quaternion conversion and `0x004F43B0`
packing use a positive-W hemisphere and truncation at `0x00407930`.
`GameObjectAnimatedPose` retains the full unscaled matrix and packed rotation
independently. `GameObjectPlacementResolver` uses the matrix for placement and
passenger positions, and the decoded packed rotation for passenger orientation.
Changes to either representation invalidate the cached placement. Replicated
movement inputs survive pose publication, and removing an entity retires its pose.

`tools/ghidra/transport_pose_oracle.py` executes all original matrix and packing
instructions for 528 synthetic poses. Tests compare every packed value exactly
and matrix components within 1e-7, then verify pose publication through an ECS
parent/passenger chain, cache invalidation, and removal.

Runtime transport clock advancement, collision registration, and player
attachment still require integration; these providers alone do not make live
transports move.
