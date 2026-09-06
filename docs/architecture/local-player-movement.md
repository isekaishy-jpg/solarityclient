# Local player movement

The production world loop now drains movement commands from its live FrameXML
state before player presentation samples ECS. `RuntimePlayerMovement` owns
held-control arbitration, a local ground/fall continuation, analytic motion
anchors, event clocks, and frozen outgoing messages. It publishes position and
movement context together, guarded by the original `WorldObjectIdentity`.

## Input and clocks

SDL event timestamps are retained as unsigned milliseconds in
`TimedPlatformEvent`; conversion divides the SDL nanosecond value before
truncating to `u32`. Coalesced mouse motion retains the last source event's
timestamp. The application installs this clock before focused UI and binding
callbacks run. World `GetTime` and movement/time-sync clocks use SDL's epoch.

The 26 movement Lua wrappers ignore their arguments and capture the retained
input-event clock, as the original wrappers read `0x00B499A4`. Trusted FrameXML
can also invoke these functions outside an immediate hardware callback; the
last input clock remains in effect. The native class-zero taint gate rejects
untrusted calls independently of hardware provenance. The current runtime
executes trusted built-in manifests; future AddOn execution still needs that
taint boundary.

`PlayerInputState` retains held controls independently of active movement axes.
Forward/backward arbitration includes autorun and the native opposing-input
sum; strafe and yaw preserve the native active-axis start/stop rules. The
runtime consumes ordered typed commands instead of binding physical WASD keys
directly to displacement. The existing binding router delivers releases even
when focused UI, the console, or loading presentation captures new presses.

## Motion and output

Ground motion samples an absolute trajectory from a retained anchor. Native
basis selection, walk/backward speeds, turning arcs, and the unsigned clock's
rounding boundaries live in `MovementGroundTrajectory`. Airborne motion retains
its launch direction and speed, and its vertical curve keeps the height offset
and analytic distance extended until the collision vector is stored.

The owner splits work at admitted command timestamps and movement heartbeat
deadlines, caps each frame's elapsed work at 250 ms, and uses the mutable runtime
geometry provider for ground, step, and airborne sweeps. Unavailable geometry
preserves solver partial state and postpones the analytic clock and heartbeat
as specified by the returned skipped interval. Invalid geometry remains an
error. Landing applies the native deferred stop/start order and rebuilds the
motion anchor. Received movement/transform changes supersede the last locally
published snapshot; world/object lifetime replacement discards stale ownership.

Outgoing messages capture event-time position, flags, and fall context before
entering the asynchronous writer. Backpressure retains the exact message and
its order. The same writer now supports `0x02CE` (packed mover GUID and skipped
milliseconds) and `0x0101` (stand-state request) alongside movement and time-sync
traffic. The wire methods do not establish unit eligibility themselves.

## Client control and local stance

`SMSG_CLIENT_CONTROL_UPDATE` (`0x159`) updates a unit's client-control bit and
the selected movement subject. The native packet uses a packed GUID and a
nonzero Boolean byte. Unknown subjects share the single pending slot from
`0x00716060`; a different disabled subject cannot displace an enabled pending
subject. Object creation resolves this slot, and world replacement resets it.

Player control notifications precede selected-mover changes. The composition
root queues the control change, dispatches `PLAYER_CONTROL_LOST` or
`PLAYER_CONTROL_GAINED` in the live FrameXML state, and collects any movement
commands from that callback before queuing the following mover change. The
original `0x00520FE0` suppresses duplicate notifications; its cursor-item cleanup
does not release physical keyboard bindings.

The movement owner stops a retired local subject, freezes `0x2D1`, and sends
`0x26A` when selecting a nonzero subject. These two envelopes differ: `0x26A`
contains a full eight-byte GUID, while the original `0x0071EF80` writes a packed
GUID before the retired mover's MovementInfo. On local control recovery,
`0x006EE870` queues event 9; `0x0098B710` starts a zero-launch fall where admitted
and sends a heartbeat. The next geometry interval resolves support. This also
prevents recovery in midair from leaving a stationary player suspended.

`SMSG_STANDSTATE_UPDATE` (`0x29D`) retains its raw byte in
`PlayerLocalStandState`. This component represents Player_C's `+0x1920` value;
replicated `UNIT_FIELD_BYTES_1` remains independent. Admitted local requests
update the private state and queue `0x101`; movement that stands the player up
queues that stance request before its movement snapshot. Server-directed
standing refreshes held input, following `0x006E2B30`.

The local player's `UnitAnimationBehavior` retains its primary M2 timer across
character texture/equipment and GPU placement replacement. Posture changes
follow `0x0073F060` before the ordinary selector at `0x0071E1F0`; completion
at `0x0073B510` selects sit, sleep, kneel, submerged, and corpse successors from
the actual resolved AnimationData behavior. The scene advances this owner
before culling, and visible drawing consumes the prepared sample once.
Captured native decisions cover 2,144 admitted selection, changed-stand, and
completion cases. Archive-decoded runtime tests cover interruptions, missing
poses, callback timing, variation rolls, and retained playback.

The same owner drives remote players and creatures from their replicated
stand fields. Their world-object lifetimes retain playback across material
changes; neighbor arrivals no longer recreate existing CPU/GPU representations.
Health/flag/effect-based death admission beyond stand state 7, vehicle and
cast/emote layering, the water-height death probe, and cast/cinematic stance
eligibility still require integration. Offscreen animation-event/effect
dispatch remains separate from the primary completion prepass.

## Evidence and limits

Original executable captures cover 3,456 ground-trajectory combinations,
12,288 held-input resolver combinations, 2,304 deferred landing flag
combinations, 1,536 ground/airborne yaw samples, and 288 extended
vertical-displacement samples. Checked-in
generators require the fingerprinted locally owned build-12340 executable.
The input fixture supplies unit readiness/vehicle answers and intercepts
queued unit callbacks; it does not validate the complete admission owner.
The deferred fixture excludes selected-input camera callbacks and intercepts
only the unrelated `0x005EEB70` observer.

Additional control fixtures capture 144 original pending-slot decisions, eight
original outgoing GUID envelopes, 76 original acquire-fall flag responses, and
288 original release responses. Acquire captures execute the original callees
with no active spline. Release captures substitute the TLS clock accessor and
suppress unit notifications; they verify movement flags, not those notification
side effects. Runtime continuation tests cover the supported ground/fall modes;
pitched motion is checked only at the captured flag-response boundary.

Runtime tests exercise fine/coarse ground frames, jump release through landing,
and unavailable geometry with heartbeat postponement. Live FrameXML tests
verify ordered command capture across clock wrap and ignored Lua arguments.
Encrypted TCP tests verify sparse/zero packed GUIDs, integer skipped time,
stand-state requests, and continued decoding of a subsequent movement packet.
Control regressions also cover encrypted active/retired mover ordering,
pending control before player creation, duplicate notifications, independent
local/replicated stance, and control recovery through landing.
Existing archive-backed geometry and native ground/fall response fixtures
remain separate evidence for those boundaries.

This is an initial production ground/fall owner, not complete world movement
parity. Attached transport, swimming, flight, spline and other response modes
still require their owners and currently return an explicit unsupported-mode
error. Remote controlled-subject selection, spline/effect queues during control
changes, cinematic/vehicle admission, authoritative event reconciliation,
stance eligibility and immediate presentation, physical mouse
steering, ceiling/contact side effects, landing notification suppression, and
body-height retention across model changes remain incomplete. The ordinary
query mask assumes the client control enabled by native player entry. Initial
`0x26A` currently waits for movement-owner readiness; native entry sends it
earlier. These
tests do not establish an end-to-end live-server movement pass, camera parity,
or the requested frame-rate target.
