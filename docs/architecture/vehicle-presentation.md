# Vehicle presentation

Vehicle creation state, passenger movement frames, entry-opacity seat lookup,
and settled animated seat placement are connected. Local and remote boarding/exit
motion consumes the passenger controller, including entry timing while the
passenger model is absent. Remaining controller consumers and vehicle camera
presentation remain open under
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
active-mover boarding/exit admission, vehicle pitch updates,
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
replace it. Seated primary/upper-body routing uses the shared model scan described below.

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

Remaining gaps include launch flags, exceptional
transfer/removal callbacks and vehicle camera work. Combined live vehicle parity
and performance gains are not established by these fixtures.

The remote-transition slice passed all 1,242 locked workspace tests, with 23
archive-dependent tests ignored. Its runtime portion passed 282 tests with 18 ignored. The new
fixtures account for 3,072 exact arithmetic records, independently of the
existing 512 settled-seat matrices and 3,072 seat-lookup/opacity records.

## Entry timing before the passenger model arrives

`749E40` requires a ready parent model and a seat row to call `7493B0`; it does
not require the passenger model. `748400` leaves the passenger anchor lookup
pending while that model is absent. Until an anchor is available, the entry
target uses the raw seat offset. An absent parent model still selects ordinary
unit position. `6E6F80` selects the parent's mount before its body.

The attachment query `831410 -> 830DC0 -> 82F0F0` samples already selected bone
timers at the current scene tick. It does not run the primary completion callback
or select another variation. Initial travel duration combines these current
bones with the preceding model placement and the current movement frame.

The runtime now separates passenger input admission from timing. After local,
creature and remote model residency updates, active controllers without a current
GPU model can query the resident parent and initialize travel. This runs before
drawing and checks complete object identities directly against current model
placements, since the visibility index may still describe the preceding frame.
Loaded passenger models retain their existing pre-animation timing pass. Both
paths use a read-only selected clock for initial seat queries. Controller and
bone-palette buffers are reused; queries do not dispatch model events or consume
the CRT random stream.

A Vulkan regression covers a body or mount parent, both already resident and
arriving at the delay boundary. Distance-dependent durations verify the current
animated seat target. The passenger's later model arrival preserves the existing
deadline and travel position, and retries its authored static anchor lookup.
A separate playback regression queries overdue variations repeatedly, then
checks the later completion callbacks, event intervals and random sequence
against an owner that received no attachment queries. Paused primary pose and
continuing global time are also covered. These checks do not establish combined
live vehicle parity or a performance improvement.

The change passes 1,253 locked workspace tests with 23 archive-dependent tests
ignored, including 292 runtime tests with 18 ignored. Workspace/all-target Clippy
with warnings denied, formatting and whitespace checks pass.

## Seated body and upper animation ownership

VehicleSeat columns 15/16 and 17/18 now supply the seated body and secondary
initial/loop pairs. `747B20` selects the body under flag 2; `747BD0` selects
the secondary under flag 4. `748560` places delay/travel primaries before the
ordinary movement resolver, while `7485B0` supplies a seated secondary after
movement and posture. `7385C0` forces an alive passenger's authored body pose
and copies a non-Stand ordinary request into the upper slot. `737EF0` commits
that upper request first, using model key 4, then 6, when available.

The CPU controller retains completion bits 2 and 4 independently. `7484E0`
uses the root and key 26 for body completion and other keys for upper completion
when both seated lanes exist; otherwise it marks both bits. Phase initialization
clears the bits before and after its animation request, except for the retained
delay-to-travel cases. Model replacement starts fresh model timers while keeping
the passenger's phase and completion bits.

`M2Playback` now owns explicit bone slots and runs the shared native callback
scan, rather than advancing each slot independently. Activation order, tied
callbacks, event ancestry, terminal completion and variation RNG are covered by
[the model scan evidence](m2-animation.md#shared-active-bone-callback-scan).
Bone queries, visible poses, queued event positions and detached model playback
consume these same timers. Wounds occupy the bone's previous-pose slot and blend
over an active seated secondary. Clearing an upper primary removes its callbacks
immediately and retains the native 150 ms pose fade when requested.

`vehicle_animation_oracle.py` supplies 5,120 original-instruction selection and
completion records, SHA-256
`d1100ad2b7c7a26165166ec25558b5393fb5dadcc1214a24eac314732f143cc3`.
A decoded runtime fixture verifies different body/upper durations, separate
initial-to-loop callbacks, upper-before-body selection rolls, bone composition,
and model replacement retaining both completed lanes. These checks do not prove
the unfinished active-spell/vehicle-control providers, complete transfer lifecycle,
combined live vehicle presentation, or a performance improvement. Unit body
effect callbacks now run synchronously in the shared scan. Seat queries sample
timers without consuming events or variation rolls, and callback placement
refreshes animated parents before their attached children. The remaining generic
model consumers and scene registration order are described in the model scan
evidence above.

## Vehicle-owned ride clips

VehicleSeat's own entry/exit/ride clips are separate from the passenger's body
and secondary clips. The catalog preserves rows +84/+88/+8C and their key-bone
selectors at +90/+94/+98. Entering seated phase with flag `0x20000` submits the
ride clip to the vehicle's actual model (mount first), then records its passenger
owner. Leaving seated phase releases that registration. These commands retain
their order through passenger model loading and use the complete parent identity.

Native `756F80` accepts raw animation IDs below 506 and normalizes unsigned keys
above 34 to root key 26. `756D10` stores at most sixteen owners, including repeated
GUIDs; a seventeenth model request still succeeds even though registration fails.
`756CD0` excludes a literal selector 26 before normalization. `7577E0` removes all
records for the departing GUID and releases the requested key only when no other
record owns it. Clearing a non-root key retains its pose fade. A root resumes the
current Unit_C request.

Body and mount callbacks now route owned keys through `757280`'s seated-owner
policy. Original consumer `747980` remains active while the passenger exists.
Retired identities cannot remain active just because a detached model retains
their CPU state. Active ownership replays the completed clip with variation -1,
speed 1 and the exact completion overrun. The replay's own interruption cannot
clear ownership. A normal external interruption clears the owned bit while
retaining surviving passenger records. An owned upper key blocks ordinary upper
selection; ownership is not a blanket lock on root animation requests. Bone-key
aliases use the actual bone's callback key and timer.

`vehicle_animation_owner_oracle.py` executes original `756D10`, `756CD0`, `757280`
and `747980` for 150 table cases. GUID residency is supplied; model replay, key
release and ordinary Unit_C resume are terminal capture boundaries. Runtime
table comparisons therefore establish registration, masks, pruning and dispatch,
not the skipped model consumers. Decoded model tests exercise those consumers,
including replay phase, shared-key release, interruption, capacity and root
resumption. Renderer coverage exercises an unloaded passenger, model arrival,
two owners sharing a key, and departure on both vehicle body and mount models.

Entry/exit action ownership (`74BE10` / `749D50`), its pending spell/timeout
consumer, animation redirect bit `F60 +10:0x800`, and complete transfer/destruction
hooks remain open. Ordinary nested seat attachment does not imply that redirect.
Combined live travel and performance validation remain open.

## Testing package

Build **000102** (`0.0.3a`) installs source revision
`21b2f4126cce434fb0620eb0aa2d3f6317d38990` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`4a5de3623851ff70e6a6f6889dc28adc1e21aac3deccd3e73552715102a8b27a`.
It includes vehicle creation state, resolved seat entry opacity, native unit
passenger frames, final local/remote world projection and settled animated
seats with mounted riders and shared lighting/shadow/sorting ancestry. Remote
boarding/exit delays, arcs, primary animation loops and retained transition
state are included. Local server paths now retain camera and held input,
acknowledge active completion, and send transport changes for parent and seat
changes. Input observes the shared passenger delay/travel phases. Unloaded
passengers use current resident-parent bones to initialize travel and preserve
that deadline when their model arrives. Initial seat queries retain animation
event intervals, completion callbacks and the CRT random sequence.
Seated body and upper animations now have independent initial/loop completion
state and a shared model callback scan. Upper primaries, wound blends and clear
fades use the same retained bone slots through model replacement.
Unit authored effect construction now occurs inside the shared callback scan,
before the next callback or unit model. Attachment subtrees retain native
traversal order; seat queries sample timers without consuming callbacks and
attached callback poses refresh after parent updates.
Mounts now construct native default primaries and retain native timers through
movement updates. Their authored events dispatch before the rider, preserving
mount event positions while resolving breath attachments through the unit body.
Mount creation and movement updates use their own authored stride speed and
outgoing variation phase. Rate changes beyond the native 0.01 tolerance submit
a new weighted primary; unchanged requests retain the timer and random stream.

Queued mount jump and landing requests now execute before rider slots. Ordinary
mount completion runs before culling, with independent corpse/variation policy
and the native body overrides. Body replacement preserves the mount timer and
pending requests; dismount consumes earlier mounted requests in order.

The locked workspace run passed 1,274 tests with 23 archive-dependent
tests ignored; runtime passed 311 with 18 ignored. A final targeted run passed
23 mount checks, including an additional expired-record boundary test.
Three explicit stock archive
tests also pass, covering character movement, jump variations and drowning/death
playback, plus the stock water-effect prepare/simulate/retire test. Native scene
traversal matches 480 captured cases; a GPU regression checks callback ordering
and current moved positions for offscreen units. Another 48 original-instruction
cases verify body/mount effect-factory binding; renderer checks use distinct
mount/body event points and opposite breath attachments. Mount timer tests retain
weighted selections and replace constructor fallback modes for forward movement.
Another 240 native mount-commit cases establish the rate threshold and submitted
arguments. Timer tests cover phase and random draws; GPU tests verify mount
creation and speed changes for local players, remote players and creatures.
Another 504 native dispatch records cover model completion flags, body/mount
admission masks and live/dead routing. Runtime comparisons check all 56 ordinary
active-record cases against independent body/upper/mount timers. An offscreen GPU
test verifies jump/loop/landing/idle progression and exact shared random draws for
local players, remote players and creatures.
Workspace/all-target Clippy with warnings denied, formatting and
whitespace checks pass. Vehicle active mover selection, active spell/control
providers, transfer lifecycle, special cameras and combined live travel remain
open. Vehicle-controlled mount completion, flight/action latches, weapon-ready
conversion, remaining action-priority providers, generic model
event consumers, native scene registration order and sound callback age remain
separate integration work.
This package does not establish a performance improvement or the 1,200 FPS target.
