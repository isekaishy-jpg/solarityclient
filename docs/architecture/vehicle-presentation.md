# Vehicle presentation

Vehicle creation state, passenger movement frames, entry-opacity seat lookup,
and settled animated seat placement are connected. Remote boarding/exit motion
now consumes the passenger controller. Local-player admission, remaining
controller consumers and vehicle camera presentation remain open under
[world completion](world-completion.md).

## Native evidence

The pinned build-12340 `4D3890` reader consumes the flag-`0x80` create tail as
Vehicle.dbc ID at movement snapshot `+2C4` and initial facing at `+2C8`.
`73F660 -> 73C260 -> 74C750` creates a UnitVehicle_C owner at unit `+F5C`.
`757FA0` retains the vehicle row and initial facing. `7580F0` changes the row
without replacing the saved angle; an absent row does not delete the owner.
The constructor's fallback virtual `+38` is `6E6F60`, which returns the raw
movement facing at `+20`; it is not the pitch getter. Virtual `+34` (`6E6F40`)
converts that facing to world space through `4F42A0`.

`5D3340 -> 756EC0` resolves the parent unit's vehicle owner, bounds the seat
byte to `0..7`, reads the corresponding Vehicle.dbc column `6..13`, and looks
up VehicleSeat.dbc through its indexed row bank. Missing owners, missing rows,
out-of-range IDs and sparse holes return no seat. The raw tables have 40/58
fields and 160/232-byte records respectively.

`716650` permits entry interpolation during a parent opacity transition when
the resolved seat's signed attachment ID at `+8` is negative. The flags at
`+4` do not control this branch. The earlier isolated entry-policy fixture
called the injected `+8` word `seat_flags`; the signed input is now named
`seat_attachment_id`. The original captured arithmetic remains unchanged.
The passenger pose publisher `74A7F0` independently uses the same `+8` field
for attachment selection through the `A2D3F0` remap before `7490F0`.

`757EF0` also maintains an available-seat mask from nonzero authored slots and
the parent's live passenger list. That list and availability UI are not yet
projected by this implementation.

## Implemented path

The network decoder retains the complete eight-byte payload. The admitted
Unit/Player create path installs `UnitVehicle` before its optional movement
projection. Ordinary movement and create blocks without the vehicle flag leave
it intact. Existing remote-object creation guards still apply. New object
identities own fresh components, and a non-unit cannot acquire this owner.

`VehicleCatalog` validates both exact schemas, sorts unique primary keys and
preserves empty and unresolved seat references. Startup loads the tables from
the configured archive stack. Runtime unit model admission resolves the parent's
vehicle ID and passenger seat byte and passes the signed attachment ID into
the existing opacity rule. An already selected display keeps its fade timing.
The table joins add no work to that display's settled opacity path.

## Validation

`tools/ghidra/vehicle_seat_oracle.py` executes the native seat lookup and entry
policy without hooking the seat getter. Its 3,072 captured cases cover every
seat-byte value, absent owners and rows, sparse and out-of-range seat references,
missing parents and both parent transition states. The two authored seat records
deliberately give the flags and attachment words opposite signs.
The checked fixture SHA-256 is
`d62ff263c556e1cd854e078d56deff76e06617d53a59440229736c17c401e643`.

Runtime comparisons pass for every captured case. Encrypted packet tests cover
payload alignment with rotation/update fields, distinct living pitch and vehicle
facing values, truncation, trailing bytes,
duplicate remote creates, local definition changes, missing/zero IDs, non-unit
admission and lifetime replacement. A model-residency test verifies immediate
versus interpolated passenger opacity, the midpoint, retained timing after a
definition change and fresh admission after GUID reuse.

The configured ChromieCraft archive stack loads all 412 vehicle rows and 720
seat rows through the typed catalog, with no unresolved nonzero seat references.
It contains 27 seats with negative attachment IDs. Reproduce this table check
with `cargo run --locked -p solarity-asset --example inspect_vehicles -- <Data>`.

Build 93's full locked workspace validation passed 1,232 tests, with 23
archive-dependent tests ignored. Its runtime portion passed 275 tests, with
18 ignored. The real-archive inspection above also passed.

## Passenger movement

Movement geometry now resolves Unit/Player parents alongside GameObjects.
`UnitPassengerFrames` retains vehicle matrices and admitted ancestor lifetimes,
using the native unscaled yaw frame for generic units and the cached frame for
owners with a valid Vehicle.dbc row. The creation matrix uses world position and
world facing, independently of the retained initial-facing lane. ECS retains
this creation pose so later packets before the first frame cannot replace it.
A failed parent lookup preserves a valid vehicle's matrix; its separately queried facing still
uses the native missing-parent zero contribution. Missing vehicle rows take
the generic-unit branch. See [passenger coordinates](passenger-movement.md).

Local and remote movement consume this provider. After all timelines advance,
passengers publish their world projections and local camera-facing input again
without advancing analytic anchors, packet queues, or transport clocks. This
covers nested units and a parent that runs after its passenger in GUID order.
Packet-only remote modes retain their parent even without a ground/fall owner.
Cached inputs include exact local-pose, parent-matrix and facing bits; unchanged
frames reuse the result, and ancestry traversal reuses its scratch capacity.

The native matrix oracle covers 512 exact cases, including nested, pitched and
scaled ancestor matrices. Runtime integration covers local and remote riders,
a later parent, a live GameObject root, missing rows/parents, cycles, removal
and GUID reuse. These are fixture tests, not a combined live vehicle session.

## Remaining consumers

Complete VehiclePassenger_C's remaining flags and transfer lifecycle,
local-player boarding/exit admission, seated animation routing, vehicle pitch updates,
vehicle camera bounds/ancestor dispatch, and exceptional unit visibility.
Do not infer the passenger controller's flags from similarly numbered DBC flags.
Combined live entry, travel, seat switching and removal remain unverified.
This slice makes no FPS or stall-reduction claim.

## Animated seat models

The settled (`74A7F0` state 3) model path now consumes the seat's attachment
remap, offset, yaw/pitch/roll, and static passenger attachment. `7490F0` cancels
the vehicle attachment's animated scale against the passenger's own scale.
Absent attachments instead use the vehicle's unscaled world yaw frame and scale
the authored offset by its unit scale. Missing seats or parent models use the
passenger's upright world placement. These overrides bypass ordinary terrain tilt.

`748400/827460` captures the passenger model's static attachment position; it
does not sample the passenger's animated attachment bone. Native `6E6F80`
selects a mount when present, then the body. The runtime follows that choice for
both vehicle and passenger, preserving the body's separate saddle attachment.

An ancestry pass resolves nested vehicle models before culling, lights, shadows
and effect anchors, even with children earlier in the placement list. Body
sequence callbacks keep their existing update order. A parent mount clock
needed by the pass is retained for its later render consumer. One parent bone
palette serves all its seats during the pass. Attachment enable channels hide
descendants while their transforms remain attached, matching `828A00` child
traversal and the separate `831410` matrix getter.

Lighting queries resolve parent chains after source and entity-callback
publication. Shadow admission and whole-model sorting also follow vehicle
ancestry independently of insertion order. Ordinary attachments inherit the
root model's retained distance (`82F0F0 +88`) for multi-view mesh, particle and
ribbon sorting. Missing admitted parent generations
freeze the last model pose; explicit movement admission permits reattachment
after GUID reuse. Retirement/transfer callbacks and their exceptional ownership
cases remain part of the unfinished passenger controller.

`tools/ghidra/vehicle_seat_pose_oracle.py` executes original `7490F0` rotation,
scale, anchor and matrix composition instructions with supplied model lookups
and unit getters. All 512 resulting matrices match bit for bit. Fixture SHA-256:
`6f3cd3438508311e92436d94300d138551f734928b2a34cae3d1ecb3f172408e`.
Runtime Vulkan tests cover two nested animated seats with later parents,
independent passenger scales, lighting/shadow/sorting inheritance, mounted riders,
hidden ancestors, missing seats, removal and explicit reattachment. Separate
lighting tests cover later ancestors and nonmonotonic receiver registration.
These tests do not establish combined live boarding, travel or camera parity.
For this slice, the runtime library passes 277 tests with 18 archive-dependent
tests ignored. The locked asset and systems suites pass as well, including the
512 exact seat-pose cases. Workspace Clippy passes all targets with warnings denied.

## Boarding and exit execution

The remote movement timeline publishes parent/seat changes when they execute.
Queued snapshots and MonsterMove parent admission request animated transitions;
immediate corrections, object baselines and flushed ordinary snapshots settle
without them. This follows `6EA9B0`, `73C8E0` and `74B840`: the ordinary playback
and path handlers set Unit `A30 & 20000000` around parent admission. A same-parent
seat change also reaches the controller. Movement notifications retain their
previous world pose and admitted parent identity until presentation consumes them.

The per-unit controller owns phases 0 through 5 independently of its model.
Seat flags, delay and speed select immediate seating, entry delay, entry travel,
exit delay or exit travel. Secondary movement flag `40` selects the seat's
special-exit flag `8`; ordinary exit uses seat flag `8000`. Delay completion
starts travel at the current scene tick. Model replacement retains the phase,
timer, last placement and static anchor. Exit keeps the former seat while its
delay runs, then travels toward the unlinked unit's current movement position.

`74A200` initializes duration, projected parent speed, gravity and bounded arc
height. `747D70`, `747A30` and `74A7F0` supply wrapping elapsed time, seat easing,
yaw wrapping and parabolic world placement. Entry targets use `7493B0`'s separate
model/movement-frame compensation, including its near-unit-scale inverse and
the parent-transition branch. Initial duration uses the preceding rendered
parent placement; travel samples the current animated attachment. Entry and exit
initial/loop animations come from seat columns 13/14 and 26/27. Primary completion
selects the authored passenger loop before ordinary locomotion completion can
replace it. Seated primary/upper-body routing is not included in this slice.

The model timing pass precedes unit animation callbacks. The later ancestry
pass publishes the interpolated world placement before visibility, lighting,
shadows and effects. Airborne passengers have no attached-model parent; seated
passengers and an exit delay use the bone attachment's inherited state.

Two additional original-instruction fixtures check this arithmetic:

- `vehicle_transition_oracle.py`: 2,560 exact initialization/pose records,
  SHA-256 `d2cb73012386036b6374871a31eacd89dffcc3716ef299b79259e05aec6f89b7`.
- `vehicle_entry_target_oracle.py`: 512 exact entry-target records,
  SHA-256 `928619f8762f4de0188ce6046886691230d7b4d96d3dfbdad1c26ccca1e79c01`.

Runtime tests cover execution versus receipt, immediate/flush/path policies,
seat-only changes, initial-to-loop completion, model replacement during travel,
exit after unlinking, CPU timers without loaded models, GUID reuse and render
parent changes across the complete entry/exit sequence. The existing nested
seat, mounted rider, light, shadow and effect tests continue to pass.

Local-player server-path admission now retains the local camera/input owner,
acknowledges active path completion, and synchronously admits passenger phase
changes before resolving held controls. Delay/travel phases block local input
until the shared controller returns to detached or seated; a short server path
cannot resume movement during a longer exit animation. See
[local player movement](local-player-movement.md#server-authored-local-paths).

Remaining gaps include a resident parent's
bone target when the child model has never loaded, seated dual animation slots,
launch flags, exceptional transfer/removal callbacks and vehicle camera work.
The CPU-only controller currently initializes travel from the unit-position
fallback; this is exact when the parent model is absent. Combined live vehicle
parity and performance gains are not established by these fixtures.

This slice passes all 1,242 locked workspace tests, with 23 archive-dependent
tests ignored. The runtime portion passes 282 tests with 18 ignored. The new
fixtures account for 3,072 exact arithmetic records, independently of the
existing 512 settled-seat matrices and 3,072 seat-lookup/opacity records.

## Testing package

Build **000095** (`0.0.3a`) installs source revision
`34b9fd982647f186e7c4c03652bf3f8c9bbeafdd` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`4f9cddf362d0099bf310d85c44e834bb8f4336ef5f636b9d8bcb593928f439f9`.
It includes vehicle creation state, resolved seat entry opacity, native unit
passenger frames, final local/remote world projection and settled animated
seats with mounted riders and shared lighting/shadow/sorting ancestry. Remote
boarding/exit delays, arcs, primary animation loops and retained transition
state are included. Local-player admission, the remaining transition consumers,
transfer lifecycle and special-camera work remain open.
