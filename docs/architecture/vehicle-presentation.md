# Vehicle presentation

Vehicle creation state, passenger movement frames, and the entry-opacity seat
lookup are connected. Attachment animation and vehicle camera presentation
remain open under [world completion](world-completion.md).

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

The full locked workspace suite passes: 1,232 tests, with 23 archive-dependent
tests ignored. The runtime library portion passes 275 tests, with 18 ignored.
The targeted asset schema/reference tests and the real-archive inspection also pass.
Workspace Clippy passes for all targets with warnings denied.

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

Trace and connect VehiclePassenger_C's state/flags and transfer lifecycle,
seat attachment offsets and animation transitions, vehicle pitch updates,
vehicle camera bounds/ancestor dispatch, and exceptional unit visibility.
Do not infer the passenger controller's flags from similarly numbered DBC flags.
Combined live entry, travel, seat switching and removal remain unverified.
This slice makes no FPS or stall-reduction claim.

## Testing package

Build **000093** (`0.0.3a`) installs source revision
`c28ec3788954b241b145f7fb9329ad76330e0c41` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`4d2672b063bec0a00052fa289d1295a1ce4d995c723ec18b4e8ae0a9b67462c4`.
It includes vehicle creation state, resolved seat entry opacity, native unit
passenger frames and final local/remote world projection. The animated-seat,
transfer and special-camera work above remains open.
