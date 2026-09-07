# Passenger movement coordinates

The pinned build-12340 image is the behavioral source. `MovementTransportFrame`
implements `0x004C2FC0`'s rigid transpose and `0x004C2300`'s point conversion.
The reverse matrix deliberately preserves native rounding and does not use a
general inverse, even for a scaled parent. Facing is supplied independently by
the parent's virtual angle; `0x004F42A0` adds it and wraps with `0x004C5090`.

`0x0075FF90` and `0x0075F0A0` convert the passenger's foot and travel direction
before constructing world-axis collection bounds. Radius and height remain
unchanged. The airborne bound calculation retains the local current and launch
heights. After collection, native code converts vertices and normals to passenger
space, preserving face and owner order without normalizing the normals.

`RuntimeMovementGeometry::set_transport_frame` freezes this conversion for one
borrowed world context. Initial interval and refreshed sweep bounds remain in
world space; solver candidates are in passenger space. A cache hit reuses those
faces without transforming them again. Frame changes and incomplete queries
invalidate faces, owners, and coverage together. Explicit queries outside this
adapter continue to return world geometry.

`tools/ghidra/passenger_frame_oracle.py` executes the original matrix functions,
the final face-conversion loop, and the interval/cache routines. Only the parent
GUID/matrix/facing provider is controlled. Checked-in fixtures compare exact
float bits for 136 parent frames, 192 interval cases, and 480 sweep-cache cases,
including tilted and scaled bases. Existing unparented interval/cache fixtures
still run unchanged. A runtime integration test collects resident terrain through
a rotated, translated frame, then verifies local floor contact, world coverage,
owner identity, cache reuse, and invalidation on frame change.

`MovementTransportChange` implements `0x0098B850`'s retained-state conversion.
Entry uses the native reverse matrix and negative parent yaw; exit uses the
parent matrix and facing. The launch and step heights use the old current foot's
XY coordinates, with distinct native addition orders. The full travel direction
remains unnormalized; its separate horizontal launch lane is normalized only
above squared length 2^-22. Ground/yaw trajectories can be rebased without
recomputing input, speed, or turn rate, and their caller retains elapsed time.
Linear motion consumes the full converted direction; ground turns consume the
normalized horizontal lane.

`tools/ghidra/passenger_rebase_oracle.py` executes `0x0098B850` unchanged for 1,088
frame changes and follows native ground initialization, rebasing, and sampling
for 2,048 additional cases. Tests compare every output float bit, including tiny
directions, tilted/scaled frames, retained heights, and nonzero elapsed times.
The original unparented ground/yaw fixtures remain unchanged and pass.

`GameObjectPlacement::facing` retains the native virtual angle independently of
the full matrix. Parent cache keys include that angle. Runtime instances retain
the GameObject passenger placement separately from the initial map-model pose;
`object_movement_frame` resolves that exact lifetime. Native facing fixtures cover
656 cases, and runtime coverage distinguishes a scale-three passenger matrix
from the same transport's scale-one initial collision model, including removal.

Live local attachment and transport wire snapshots are integrated into the
movement owner. Native contact handling at `0x006EC7B0` is restricted to the active
mover. Admission at `0x0074B3F0` calls the candidate's virtual `+0xEC`; a GameObject
checks GAMEOBJECT_FLAGS bit 8 at `0x00712F20`. Leaving a contact can retain the old
parent through virtual `+0xF0`, which delegates to behavior `+0x6C` (`0x00712E90`).
Type-11 and type-15 behaviors use `0x0070B360` and `0x0077FFB0`: no map handle
retains the parent, while resident M2 and WMO handles use their own containment
rules. These gates must not be replaced with a generic grounded-contact rule.

`MovementTransportVolume` preserves `0x0077FFB0`/`0x007AEA10`'s inclusive
half-spaces. An M2 requires a ready model and uses its authored collision box,
with the native two-constant upper-Z padding and single float spill. A WMO
requires loaded groups and tests its MCVP planes; a loaded root with no planes
retains every point. `tools/ghidra/passenger_containment_oracle.py` captures 880
original decisions with controlled M2 readiness, including missing models,
unloaded groups, empty and oblique plane sets, and adjacent float boundaries.

The runtime exposes boarding permission and retention independently, keyed by
object lifetime. Tests cover the M2 headroom, flag removal, object removal, and
WMO authored planes. Type-15 WMO collision registration now waits for the
template to admit its map handle, matching the existing M2 admission gate.

MO behavior retains both raw route time (`+0x30`) and the converted passenger
clock (`+0x3C`, virtual `+0xA8`). The latter is the sampled phase, including
station holds and period wrapping, and remains unchanged on a next-map sample.
Runtime tests distinguish these clocks. The movement-context current/previous
clock and the optional second wire clock are retained separately by the local
movement owner. `0x006E8F70` moves current to previous before publishing the new
phase. Boarding calls this immediately through `0x0098BA20` -> `0x0074B340` ->
`0x00711AB0`; it does not wait for the next scene update. `0x006EC400` marks the
optional clock only when replacing a nonzero old parent. Leaving retains the
clock. Ordinary scene refreshes publish the attached behavior's phase, including
an unchanged phase during a stop or a sample on another map.

The local movement owner now retains its position, facing, analytic anchor,
ground basis, launch height, step height, and fall direction in passenger
coordinates. A parent switch applies the old exit conversion and then the new
entry conversion without restarting analytic time. An idle scene refresh changes
the world projection while preserving the local state. ECS and ordinary packet
fields receive the projected world transform; the transport block receives the
local transform. Camera view and free-look admission use world facing, while a
mouse-facing command converts the camera's world yaw to passenger coordinates.
Authoritative attached entry uses the transmitted local coordinates once its
parent matrix is available.

Ground contact reports static GUID zero as well as dynamic parents. The airborne
wrapper `0x007618B0` first checks the existing parent's retention volume; a leave
returns before consuming collision time. Its inner response `0x007612B0` reports
only a nonzero contact GUID. `0x00762E00` stops the interval on a parent GUID
change or landing, subtracting the unconsumed portion from the analytic clock.
Seat-only changes do not count as a new coordinate system. `0x006EB0B0` gives a
landing notification precedence over ChangeTransport, so landing onto a deck
emits one FallLand packet that already contains the new parent.

`0x00987140` synthesizes wire flag `0x200`; that flag is not retained in the
internal input flags. The optional second clock adds secondary flag `0x400` and
is consumed when a transport packet is frozen. Presentation snapshots do not
consume it. ChangeTransport on leaving still contains a transport block with
GUID zero, world-as-local coordinates, seat -1, and the retained clock. Later
ordinary packets have no transport block. The existing immutable writer queue
preserves all these fields under backpressure.

Runtime tests land on an actual resident M2 deck over ADT terrain, follow a moving
parent while idle, walk in passenger coordinates, test retention and rejected
contacts, switch between two independently clocked parents, and exercise an
airborne volume exit. Packets pass through the encrypted loopback writer and
reader before field assertions. Tests also cover authoritative local-coordinate
admission, world-facing mouse input, and the prohibition on remote auto-boarding.

Remaining integration includes type-11 animation paths, remote passenger travel,
and the transport destruction callback before object retirement. An unexpectedly
missing parent follows `0x006EC400`'s GUID-clear path without applying a stale
matrix; normal `0x0070FFD0` destruction requires its earlier detach sequence.
