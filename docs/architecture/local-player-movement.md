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

Movement notifications retain jump takeoff, airborne loops, and landing requests
independently of network-writer backpressure. The primary timer scales ordinary
locomotion by the resolved sequence's authored movement speed and preserves stride
phase across speed changes. These paths continue to use the shared stock CRT
random stream for admitted sequence variations.

The same retained owner now advances the ordinary nonspline body controller at
`0x0073DAB0`. Strafe and diagonal movement smooth body yaw independently of facing;
keyboard/admitted mouse turns twist semantic spine/head bones and release into
procedural idle-turn requests. Model placement uses the resulting body yaw. Local
bone overrides precede authored rotation/scale inside the pivot and parent chain,
so held items, attachment effects, and descendant bones inherit the composed pose.
Facing-only updates do not enqueue a primary sequence request. Neither body state
nor playback is recreated by GPU or texture replacement.

The checked-in orientation oracle captures 1,176 independent native transitions
and 1,680 consecutive frames, including 1,200 Hz stepping. Fifteen original matrix
compositions cover noncommuting rotations, nonuniform scale, pivots, translation,
and inherited spine/head overrides. Runtime checks cover mouse release, unchanged
random state during twist, and body placement through culling/resource replacement.
The installed-archive test exercises all eight directions and attachment poses on
the twenty playable race/gender models. These checks establish animation behavior;
they do not measure end-to-end frame rate.

The same owner drives remote players and creatures from their replicated
stand fields. Their world-object lifetimes retain playback across material
changes; neighbor arrivals no longer recreate existing CPU/GPU representations.
Health/flag/effect-based death admission beyond stand state 7, vehicle and
cast/emote layering, the water-height death probe, and cast/cinematic stance
eligibility still require integration. Offscreen animation-event/effect
dispatch remains separate from the primary completion prepass.
Spline orientation, vehicle body overrides, remote combat-facing targets, and
the native camera-mode-13 spine policy remain outside the ordinary body path.

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
stance eligibility and immediate presentation,
ceiling/contact side effects, landing notification suppression, and
body-height retention across model changes remain incomplete. The ordinary
query mask assumes the client control enabled by native player entry. Initial
`0x26A` currently waits for movement-owner readiness; native entry sends it
earlier. These
tests do not establish an end-to-end live-server movement pass, camera parity,
or the requested frame-rate target.

## Ordinary mouse camera input

The stock `TURNORACTION` and `CAMERAORSELECTORMOVE` Lua bindings now feed
timestamped commands into the same queue as relative mouse motion. Left drag
retains a world-space camera yaw independently of the subject; right drag
updates subject facing and queues `MSG_MOVE_SET_FACING`. Holding both buttons
uses the existing forward-input resolver. The resulting view is published to
ECS before presentation. SDL relative mode follows the admitted free-look
state, including releases generated by the binding router on focus loss.

The native `0x006020B0` mouse scaling uses the original 800-by-600 pixel basis,
the camera yaw/pitch speed CVars, and independent mouse-inversion CVars.
`tools/ghidra/camera_mouse_oracle.py` captures 112 cases from the original
instructions; runtime tests compare their float bits. The production service
regression also covers initial support acquisition, walking, left/right drag,
paired-button movement, view publication, and outgoing facing/forward events.

`IsModifiedClick` reads retained physical modifiers and the active binding
assignments. Its explicit modifier expression requires every named family;
an assigned chord accepts any assigned modifier bit, following `0x0055F940`
and `0x0055D6B0`. Pointer dispatch scopes the optional current-button filter.
The real archive FrameXML example checks menu toggling and both mouse bindings,
including the default Ctrl sticky-camera argument on release. Idle spell/target
cancellation queries allow the stock Escape chain to reach the menu; active
cast, channel, targeting, and selected-target owners still require integration.

Special controlled subjects and tracking camera modes remain incomplete.
These checks do not establish live camera parity or resolve the reported world
frame rate. The Testing installer can enable
`SOLARITY_FRAME_TIMINGS` with `-FrameTimings` to attribute slow world frames.

Ordinary wheel zoom now runs the native timed distance banks rather than an
instantaneous distance step. `CameraZoomIn` and `CameraZoomOut` retain the input
timestamp and float amount; absent/nonnumeric arguments default to one. Requests
extend an active same-direction deadline or mark the opposite direction stopped.
The frame sample applies the elapsed interval using `cameraDistanceMoveSpeed`
and clamps against `cameraDistanceMax * cameraDistanceMaxFactor`, capped at 50.
The stock zero-duration indefinite hold and wrapping timer comparisons are
preserved. Numeric CVar sampling borrows retained text without allocating in the
camera update. Existing camera pose, obstruction, and water-collision owners
consume the resulting requested distance.

`tools/ghidra/camera_zoom_oracle.py` executes original requests `0x005FF950` /
`0x005FFA60` and the distance sampler at `0x006000E0`. All 480 captured histories
match the distance float bits and both complete timer banks, including repeated
scrolls, direction reversal, clamping, and clock wrap. Production service tests
check zoom publication without changing pitch/yaw; the real FrameXML example
checks both stock wheel bindings. Special-subject distance scaling and saved-view
interpolation remain outside this ordinary zoom owner.

Ordinary relative-yaw follow now uses `0x00602760` profile selection on held
control changes. Idle, Stop, Move, Strafe and Turn select the highest applicable
condition in the active Never/Smart/Always/Spline/Smarter table. Native axis
delay and factor composition, pitch/yaw admission bounds, shortest-angle
normalization, shared duration, and repeated-request retention drive separate
pitch and yaw lanes. Entering mouse look cancels the lanes; sticky release
suppresses subsequent follow requests until an ordinary mouse release clears it.
The follow request uses the service clock, as the original samples `0x0086AE20`
at the input edge, while wheel requests retain their input timestamp.

The frame update preserves the original cosine interpolation, fraction float
store, delayed start, wrapping clock comparison and next-sample completion.
`tools/ghidra/camera_follow_oracle.py` executes original profile selection,
request helpers and interpolation instructions for 900 histories. Tests compare
every captured angle bit and retained lane word after each action; only object
lookup and the clock are supplied externally. Production service coverage checks
idle orbit retention, movement recentering without subject steering, stop
cancellation, and live selection of Never. All native condition/axis profile
CVars are registered, and the runtime only rebuilds its numeric policy after the
shared CVar generation changes. The ordinary owner still lacks saved-view
editing/gates, tracked-subject absolute-yaw modes, roll and cinematic policy;
the native CVar rejection callbacks are not yet reproduced by the registry.

`benchmark_world_camera` measures the real resident terrain/WMO/M2 obstruction
and water queries without a window or server. It accepts a Data directory,
locale, map, world X/Y/Z and sample count. The input position and synthetic
subject marker are explicit; this single-tile diagnostic excludes rendering,
FrameXML, nearby-tile streaming and replicated objects, so its results are not
world FPS measurements. Its Orgrimmar sweep exposed terrain hits beyond the
requested camera segment. Terrain trace selection now rejects a plane beyond
the current maximum fraction before publishing a contact, rather than extending
the cutoff with bounds tolerance or clamping a later plane to the endpoint.
Archive-backed regressions cover both a shortened segment and a cutoff just
before the surface.

Obstruction feedback now retains the requested zoom separately from the clipped
distance. Native `0x006076C2` feeds a significant collision into the distance
interpolation lane; `0x00603E7B` restores the requested distance with its two-second
cosine curve. The camera still moves inward on contact. Repeated obstruction,
wheel input during recovery, the original distance bias, and wrapping clocks
are covered by `camera_obstruction_recovery_oracle.py` histories. Waterline
correction is applied after the distance feedback and does not become a zoom
request.

The reported Blood Elf alternate jump has additional archive-backed coverage.
Both standing and forward-running movement execute 128 complete jumps for each
sex through the real local movement and retained animation owners. Both authored
variants survive to distinct sampled bone palettes. User testing of Build 000020
confirmed the alternate jump, with the female model's sequences 108 and 109
both present in the session log. The CRT stream and authored frequencies remain
unchanged, and each live jump records the model, selected sequence and
variation, movement flags, and scene time for the next user test.
