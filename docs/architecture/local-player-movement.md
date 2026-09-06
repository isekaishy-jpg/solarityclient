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

Runtime tests exercise fine/coarse ground frames, jump release through landing,
and unavailable geometry with heartbeat postponement. Live FrameXML tests
verify ordered command capture across clock wrap and ignored Lua arguments.
Encrypted TCP tests verify sparse/zero packed GUIDs, integer skipped time,
stand-state requests, and continued decoding of a subsequent movement packet.
Existing archive-backed geometry and native ground/fall response fixtures
remain separate evidence for those boundaries.

This is an initial production ground/fall owner, not complete world movement
parity. Attached transport, swimming, flight, spline and other response modes
still require their owners and currently return an explicit unsupported-mode
error. Client-control updates, cinematic/vehicle admission, authoritative event
reconciliation, stance eligibility and immediate presentation, physical mouse
steering, ceiling/contact side effects, landing notification suppression, and
body-height retention across model changes remain incomplete. The ordinary
query mask assumes the client control enabled by native player entry. These
tests do not establish an end-to-end live-server movement pass, camera parity,
or the requested frame-rate target.
