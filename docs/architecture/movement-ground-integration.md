# Grounded movement intervals

`MovementGroundState::advance` implements native `0x007620F0` over complete,
ordered collision candidates. It follows admitted surfaces, slides against
walls, tests steps, and returns either updated ground state or a new analytic
fall. It accepts an already-generated horizontal direction/distance and interval;
timestamped input and resident-world collection still require runtime owners.

The implementation lives in `movement/grounded`. Collision owns body sweeps,
ground-contact classification, and combined foot-plane normals. Movement owns
the repeated-contact loop, corrections, step state, and speculative fall. The
ground loop reuses the verified [fall interval](movement-fall-integration.md)
for step trials. Neither collision nor ECS depends on the movement subsystem.
These pure queries allocate no memory or mutate external state.

## State and owner actions

`MovementGroundSnapshot` retains foot position, an optional active step anchor,
the unsigned fall clock, and launch height/downward speed. It also receives the
current resolved ground input bases/speed, terminal-speed mode, and explicit
start-fall admission. The bases describe what the input owner supplies when
stock rebases or starts falling; contact sliding does not replace that input.

`MovementGroundProfile` pairs the controlled-player support policy with its
unit-owned step height. Other units use their native support policy and a
two-unit step height. The native `0x00716710` predicate must be resolved by the
caller. The start-fall gate similarly represents the result of the movement
and active-spline mode checks at `0x00988370`, independently of wire flags.

A successful fall transition clears step ownership, resets fall time and
downward launch speed, and anchors the fall curve at current height. The result
requests an analytic movement-anchor reset. A suppressed transition keeps the
ground state. Ground intervals return the original millisecond duration except
when an empty candidate set successfully starts falling, which consumes zero.
An empty set means collection completed with no candidates; collection failure
must be handled separately by the world owner.

The result also returns analytic-anchor and candidate-identity notifications.
The outer owner must apply position/facing/pitch rebasing, clear analytic elapsed
time, and resolve resource/transport identities and their subject gates. The
query does not send packets or callbacks. The native post-rebase `0x005EEB70`
is a bare RET, as recorded by the fall investigation.

## Contacts and steps

Ground classification at `0x0075D1C0` distinguishes following a slope from
support within its footprint. An admitted slope is followed even outside the
footprint; support also clears step state. Negative-Z contacts touching the body
top can clear an active step and request falling. A classification result updates
the current force-fall field rather than latching every earlier contact.

`0x0075DE80` combines one through four sloped-foot body planes. Four planes
produce a valid zero vector. Surface following at `0x0075E0C0` can use the
opposite combined foot normal, clamps vertical correction to the available
step height, and scales travel/time accordingly. Wall correction at `0x0075E250`
preserves the remaining horizontal-distance limit and native 0.001-unit bias.

The step probe at `0x00761B00` chooses a horizontal probe direction, sweeps up,
forward, and down, and can apply another surface correction while raised. It
accepts clear downward travel or a flat-enough support surface. Steeper support
requires `0x007619C0`'s speculative fall and horizontal distance/direction test.
The original step-probe position is restored before returning either decision.
Acceptance starts or retains a step anchor; rejection clears step state.

The trial computes its native rounded millisecond duration from the drop,
starts falling if admitted, and repeatedly invokes the fall interval with trial
policy. Its horizontal request is the unscaled ground basis, as in the original
instructions. Native trial restoration retains the resulting position and
launch-height/downward-speed writes while restoring clocks, bases, speed, and
flags. The surrounding step probe subsequently restores position, but those
launch writes survive. A whole-state rollback would lose this behavior.

The ground loop stops after six contacts with less than one millisecond of
progress. It also preserves the native climb budget, step-height cap, clear-travel
downward probe, and reanchoring conditions. Direction and distance remain
separate native float images at the sweep boundary. Wider intermediates survive
through corrected-vector length, normalization, wall response, and position
updates until the native store points. Early reciprocal rounding changed an
angled step's next contact; early wall-response rounding changed whether a
0.001-unit retry started falling. Both cases are covered by native captures.

## Independent validation

`movement-ground-advance-native.txt` records 10,989 complete executions of
original `0x007620F0` in build 12340, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
Capture used Unicorn 2.1.4, x87 control word `0x037F`, explicit unit images,
and complete cached candidate bounds. Every native callee executed, including
the support predicate, step/fall trials, rebasing, and notification gates.
No native function was stubbed. Checked-in data contains scalar inputs and
outputs; tests require neither the game executable nor an emulator.

Cases include the previous 69 sweep geometries, varying step heights and
approach angles, steep wedges, zero/tiny travel and large unsigned clocks,
600 deterministic contact scenes, both support profiles, active steps,
suppressed fall starts, and normal/slow fall. Another 261 successive native
frames carry step anchors and retained launch fields through translated stairs,
ceiling, wedge, and corner scenes. Sequences stop when stock starts falling or
reach their frame limit; they do not assert that every scene remains traversable.

Read-only native branch observation recorded 3,774 step probes: 846 accepted,
2,280 rejected, and 648 cleared by the zero-rise branch. There were 66 speculative
fall calls, 4,848 vertical corrections, 2,928 wall corrections, and 2,538
start-fall calls, including trial and suppressed calls. Sequence preparation
calls are excluded from these counts.

Tests compare consumed time, retained fall clock, ground/fall continuation,
step state, anchor requests, and resource notification requests exactly.
Positions, active step anchors, launch fields, and resolved bases/speed compare
within 0.0001 world units. Admission tests reject poisoned state, negative
generated travel/step height, and invalid body dimensions.

## Runtime work remaining

The ground and fall interval cores are ready for a local movement owner. They
do not yet consume keyboard timestamps, assemble the implemented
[terrain/WMO/M2 face collectors](movement-world-geometry.md) across resident owners,
transform transport coordinates, apply landing input resets, or update a living
ECS transform. Swimming/flying modes require their own native response owners.
No runtime movement, camera, or frame-rate parity claim follows from these
isolated interval tests.
