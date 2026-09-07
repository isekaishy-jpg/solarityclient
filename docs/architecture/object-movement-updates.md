# Movement-only object updates

Build-12340 `SMSG_UPDATE_OBJECT` operation 1 has a packed GUID followed directly
by the living movement block. It does not prefix `UpdateFlag`, even though
creation operations 2/3 do. Treating operation 1 as a creation movement block
shifted every subsequent read and could reject a valid packet or interpret its
movement flags as unrelated placement flags.

The native boundary is `0x004D6DA0`: packed GUID reader `0x0076DC20`, movement
initialization, and then `0x004F5090`. The latter calls MovementInfo reader
`0x004F4D40`, reads nine speed floats, and conditionally calls spline reader
`0x004F4B50` when movement bit `0x08000000` is set. Creation enters the same
reader through the living branch of `0x004D3890` after reading `UpdateFlag`.

`UpdateCursor::read_living_movement` now serves both paths. Creation retains its
own conditional tails and field mask. Movement-only snapshots report zero
creation flags and cannot manufacture non-living passenger offsets or packed
GameObject quaternions. The decoder preserves wire flags and optional field
presence; native clears INTERPOLATED after reading its second transport clock,
while the network snapshot retains that original bit and the optional clock.

`GameplaySession` consumes local-player operation-1 echoes without applying them,
matching the GUID guard in `0x004D6DA0`. Remote Unit/Player snapshots update
position, all speeds, and the full conditional context in packet order. Missing
conditional fields clear the previous snapshot's attachment, pitch, and fall
data. This operation does not replace the local input/control protocol.

Stock dispatches remote operation 1 directly to the Unit movement owner at
`0x0073C220`. The runtime explicitly rejects a non-unit target before mutating
it, preventing malformed living data from erasing a GameObject's packed rotation
or adding a living component. This category validation is a Rust admission
guard; it is not a claim that the native client gracefully rejects that malformed
target. Unknown GUIDs retain the existing lifecycle error.

## Verification and limits

`runtime/tests/fixtures/object-movement-native.txt` stores eight input payloads
and native movement structures produced by executing unmodified `0x004F5090`
and its readers. The capture used an initialized native data buffer and executed
without hooks or replacement routines. It checked exact byte consumption and
all nine speeds. Its header records the source executable SHA-256.

Encrypted loopback tests compare those snapshots with the Rust decoder. Other
cases cover creation/movement layout separation, following field operations,
local-player echoes, remote attachment removal, non-unit rejection without
mutation, conditional spline consumption, and every prefix truncation of a
populated movement packet followed by a valid encrypted frame. The deliberately
incorrect creation-prefix layout is rejected.

The native reader fixture corpus excludes spline-enabled blocks; independently
authored encrypted fixtures check every retained spline field and packet tail.
`MovementSplineSnapshot` preserves the complete final-facing union, clocks,
scales, controls, mode byte, effect time, and separate destination.

The systems-owned `MovementSpline` component installs these controls using
`receipt - elapsed` (`004F4B50`). Installation selects linear or Catmull-Rom
geometry from flags `0x42000`, as `006F1520` does. `MovementPath` uses the original
four-control windows, distance-weighted segment selection, and twenty-sample
smooth length cache (`004C3980`, vtable `009E2F28`). The native geometry fixture
contains 200 position/direction samples, including long and degenerate paths.

The runtime remote movement phase runs before model and sound synchronization.
It takes ownership of the admitted spline and publishes its compact movement
summary to ECS. `advance_world_movement_splines` also exposes standalone path
playback for consumers that do not own an ordinary command timeline.
The path clock follows `0098CA00`, including unsigned clock wraparound, scaled
durations, frozen/reversed paths, one cycle per evaluation, and the initial-cycle
control replacement at `0098C940`. Endpoint placement/facing and movement stop
follow `006EB0B0`/`0098BD10`. Its 320 native evaluator fixtures also cover falling
and parabolic vertical offsets; dedicated tests check final target facing and
the initial-cycle replacement. Fixtures compare floats within 1e-5 absolute or
1e-6 relative tolerance; they do not claim bit-exact x87 emulation.

`WorldMovementSpline` publishes only the live flags, cached length, authored
duration, and effect admission to animation. `Movement::GetSpeed(0)` (`987570`)
uses path length divided by authored duration for an active spline, allowing
the existing native walk/run threshold to select walking for slow NPC paths.
Encrypted integration tests check intermediate placement, walking, endpoint
orientation, stopping, path replacement, and removal by a nonspline snapshot.

Post-spawn `SMSG_MONSTER_MOVE` (`0xDD`) is decoded and dispatched through the
same retained owner. The protocol also decodes the transport form (`0x2AE`),
whose attachment controller remains a prerequisite for applying those points.
Native `0073F590`/`0073C8E0` define the header, stop/facing forms, conditional
effects, and full or packed point arrays. Packed offsets use signed 11/11/10-bit
quarter-yard coordinates relative to the start/destination midpoint (`007152B0`).

`MovementPathRequest::prepare` follows `0073C8E0` and `007180C0`: it reconciles
linear starts against the current unit position, builds endpoint controls, and
limits speed to `max(28, 4 * run_speed)` or 50 for smooth paths. Its duration uses
the original chord-length calculation and nearest-even integer conversion.
Short stops place immediately inside `pathDistTol` (native default one yard,
registration `00715330`); larger corrections traverse a short path. A corpus of
540 complete native preparations checks decoded points, controls, placement
decisions, and effective durations. Its controlled tolerance is four yards.
Encrypted tests cover idle-to-walk, replacing an advancing path, stopping, and
late commands after GUID removal. Unknown units are not manufactured by a path.

Ordinary incoming movement commands use the original `00741B60` GUID guard and
`006EB730` receipt/server clock admission. The retained history, delay clamps,
wrapping comparisons, and immediate/future decisions are checked against native
execution. Future commands retain stable timestamp order. `006EA6A0`/`006EA7E0`
position and angle correction are sampled before ground/fall collision; a
separate native corpus checks their numeric results. Normal frames cap movement
at 250 ms and split at queued timestamps (`006F09F0`); immediate corrections
catch up in 250 ms chunks (`006EA550`). Rooted translation is discarded by the
`006EF860` command gate. Idle units do not initiate terrain simulation, and
`00406DE0`'s two-yard map margin prevents out-of-map integration.

Object baselines, ordinary commands, and monster paths share an ECS inbox.
Before path replacement, `006ED7E0`/`006ED0F0` flush future snapshots without
replaying ordinary jump/axis actions. Retaining each baseline's own path avoids
later packets overwriting geometry before its turn in the queue. World/entity
identity prevents GUID reuse from retaining an old command or trajectory.
Encrypted loopback tests exercise NPC walking, remote-player running, stopping,
local echoes, unknown GUIDs, and GUID reuse through dispatch and resident terrain
to ECS publication. Regression cases cover mixed ordinary/path ordering,
heartbeat landing, unavailable geometry, roots, and clock wrap.

Transport path frames and special spline collision/event policies still require
their native owners. Swimming, flight, mounts, and vehicles remain backlog work
with their prerequisites. Ordinary pitch-arc trajectories (`00987950`) retain
authoritative snapshots until the three-dimensional solver is present; the
horizontal predictor does not simulate them. Separate speed, root, and movement
effect opcode families also require their command owners. Transport paths
are retained but are not published as world coordinates before that attachment
controller exists. These tests do not establish live visual parity or complete
remote movement behavior.
