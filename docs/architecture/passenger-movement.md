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

Transport destruction follows `0x0070FFD0` before presentation releases the old
generation's matrix. It clears the special movement flag (`0x009872B0`), detaches
through the live parent frame, and sends the forced GUID-zero leave. The active
mover also receives `0x006ECCF0`'s queued support recheck, inserted with
`0x006EC090`'s stable wrapping-timestamp order. That event uses the existing
zero-launch fall admission; it does not restart an already admitted fall. A
position correction received before removal supplies the local pose to convert.
World replacement or a server-authored change to another parent suppresses the
old link's notification. Tests release the actual parent resource between the
leave packet and support recheck and inspect both encrypted notifications.

An unexpectedly missing parent still follows `0x006EC400`'s GUID-clear path
without applying a stale matrix.

## Ordinary remote passengers

The remote timeline admits ordinary ground/fall passengers into the same local
coordinate integrator. `0x006EB730` first installs the ordinary world snapshot
through `0x00988920`; `0x009872C0` then substitutes the transmitted local position
and orientation only if the nonzero parent resolves. An unresolved immediate
parent therefore leaves the world pose with GUID zero. Parent frames refresh
even when no time elapsed or the unit has no movement axes, so a stationary
passenger follows its deck without probing the ground.

Queued snapshots use the local position/orientation selected by `0x006E9050`.
`0x006EA6A0` allows interpolation only between matching parent GUIDs, and both
lookahead endpoints and analytic predictions remain in that parent's coordinate
space. The wire `0x200` transport flag is excluded from `0x006EF860`'s internal
immobilization gate, allowing queued starts and stops on deck. `0x006EAC40` also
tests map bounds against the movement owner's local coordinate lane.

Before applying a queued pose, `0x006EA9B0` calls `0x006EA1D0` to change the link.
That change rotates a retained airborne direction through the old and new parent
frames without replacing launch speed. Failed new admission unlinks the old
parent and leaves the previous local pose and flags untouched; it does not take
the immediate path's world-pose branch. Remote projection retains received
transport clock metadata; it never publishes the active mover's TLS clock.

Tests use the real resident route and collision deck to cover idle frame changes,
interpolated walking/stopping, both missing-parent policies, and a queued airborne
switch between independently clocked transports.

## Transport-authored unit paths

Both setup and live dispatch admit `SMSG_MONSTER_MOVE_TRANSPORT` into the same
receipt-ordered inbox as ordinary remote snapshots. `0x0073C8E0` flushes earlier
commands (`0x006ED7E0`) before forcing the requested parent through
`0x006F0C70`/`0x006EC400`. Unlike queued ordinary snapshot admission, this parent
change rejects an unavailable nonzero parent before unlinking the old one. The
caller compares the resulting GUID and does not replace the path on failure.
Ordinary axes are already stopped by that point; an active spline retains its
axes and geometry.

The runtime prepares and reconciles controls in parent coordinates. It uses the
retained local pose for a same-parent replacement and converts world placement
when changing parents. Each retained path owns a parent identity and refreshed
matrix; evaluation and endpoint facing use local coordinates, including the
conversion of a target's world position. `0x006E9470` stores the local result and
`0x004F4460` projects it for world consumers. Completed paths continue to follow
their parent while idle. A subsequent world-space path first detaches through
the current parent projection. Creation snapshots also seed a falling path from
the transmitted local height instead of its ordinary world height.

Resident-deck tests cover local traversal, target facing, continued parent motion
after completion, rejected parent admission, and replacement by a world path.
Encrypted fixtures cover both dispatcher branches and falling creation snapshots.
The standalone world-space path API delegates attached travel to this runtime
provider instead of publishing local coordinates directly.

Remaining transport integration includes remote destruction callbacks and
type-11 animation paths.

## Unit and vehicle parent frames

Unit/Player virtual `+C4` (`722B50`) returns Vehicle_C `+10` when the unit has
a vehicle owner and a valid database row. Otherwise it constructs translation
from raw local position (`+30`), rotates by raw local facing (`+38`), and composes
the parent `+C4` matrix. Unit model scale, pitch and seat bone animation are
independent of this base frame. Virtual `+EC` (`8A1420`) and `+F0` (`959DE0`)
both return true. Virtual `+F4` (`74B810 -> 757980`) maintains the passenger list;
it does not publish the GameObject transport clock.

`757FA0` seeds a vehicle matrix from the creation world pose. `73AB20 -> 758130`
refreshes it from the unit's current local pose and resolved parent matrix;
failed parent lookup leaves it unchanged. `757BE0` propagates matrix changes
through nested vehicle passengers, and `7132E0` supplies a moving GameObject's
matrix to vehicle passengers. Facing remains a separate getter: `4F42A0` adds
parent world facing; `74B590` contributes zero for a missing parent and the
caller still wraps the sum.

`MovementTransportFrame::unit` follows `4C3380 -> 4C3290/4C1F00` and `4C2370`.
The sixteen matrix entries preserve their distinct native addition orders.
`tools/ghidra/unit_passenger_frame_oracle.py` executes these original functions
without hooks and checks 512 records, including signed zero and nested pitched
or scaled matrices. Fixture SHA-256:
`755401fe132cefe1cab3472e83e6e34911aa0b636e18572a06acde7d06191a02`.

The runtime shares `UnitPassengerFrames` between local movement, remote
timelines and final projection. Parent links retain full object lifetimes;
cycles are rejected, and GUID reuse cannot replace an admitted ancestor.
An explicit movement-owner reattachment records the new parent lifetime,
including removal and reattachment within one receive batch. The creation
world pose is retained in ECS before subsequent packets can overwrite it.
All local poses finish advancing before the final passenger world projections
are published. This makes nested travel independent of GUID iteration order
without resimulating motion or republishing the TLS clock. The local camera
view receives the same final parent-facing projection. Runtime tests cover a
moving later parent, nested vehicles, packet-only remote passengers, a real
GameObject deck, missing database rows and ancestor lifetime replacement.

Animated seats and remote/local server-path entry and exit are connected; see
[vehicle presentation](vehicle-presentation.md) and
[local server paths](local-player-movement.md#server-authored-local-paths).
Vehicle passenger transfer/destruction callbacks, special vehicle camera policy
and combined live travel remain open.
