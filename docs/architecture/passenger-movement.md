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

Live attachment, parent retention, and transport wire snapshots still require
runtime integration. Native contact handling at `0x006EC7B0` is restricted to the active
mover. Admission at `0x0074B3F0` calls the candidate's virtual `+0xEC`; a GameObject
checks GAMEOBJECT_FLAGS bit 8 at `0x00712F20`. Leaving a contact can retain the old
parent through virtual `+0xF0`, which delegates to behavior `+0x6C` (`0x00712E90`).
Type-11 and type-15 behaviors use `0x0070B360` and `0x0077FFB0`: no map handle
retains the parent, while resident M2 and WMO handles use their own containment
rules. These gates must not be replaced with a generic grounded-contact rule.
