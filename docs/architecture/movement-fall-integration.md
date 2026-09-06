# Falling movement intervals

`movement/airborne` now owns the fall-specific state updates at native
`0x007612B0`. `MovementFallState::advance` combines the body sweep, fall contact
response, and analytic height crossings across an entire supplied interval.
It returns updated airborne state or a typed ground transition, together with
the consumed time and movement-owner actions. It allocates no memory.

The caller supplies local fall state, an already-generated displacement and
duration, resolved body dimensions/support policy, and complete ordered
collision candidates in one coordinate space. These inputs are independent of
received movement flags. Resident-world collection, transport conversion, and
timestamped keyboard integration still need their runtime owners.

## State and continuation

The admitted snapshot retains current foot position, unsigned fall time,
launch height and signed downward speed, horizontal direction/speed, the full
movement basis, terminal-speed mode, and FALLING/FALLING_FAR phase. It preserves
already-resolved bases, including tiny vectors; admission does not normalize
or replace them.

Each contact advances position and consumed time before evaluating its phase
transition. An upward ceiling contact sets FALLING_FAR, resets fall time and
vertical launch speed to zero, and starts the next fall curve at the new height.
A support contact ends the active fall. The `Landed` continuation carries the
position and the retained fall clock; fall time remains unconditional on wire
even after FALLING clears.

The ground/input owner must finish `0x00988490`'s non-fall responsibilities:
clear the local falling flags, rebase position/facing/pitch and elapsed time,
rebuild the current input direction, select ground speed, and process the
pending input handled by `0x006EB3B0`. Returning a ground transition does not
substitute an assumed movement speed or rewrite opaque wire flags.

## Repeated contacts

The loop retains the original direction and remaining travel for each contact.
A side response adds the native XY correction while retaining vertical motion,
then projects horizontal launch speed onto the new direction. Negative projected
speed becomes zero. The full movement basis follows the resulting horizontal
basis with Z zero, as at `0x0076166D` onward.

Progress at or below the exact 0.0005-second constant at `0x00A25098` increments
the native small-progress counter. After six such iterations, stock retries
vertically with horizontal speed zero. A second exhausted retry, or one with
negligible horizontal travel, requests end-fall. A progressing contact resets
the counter to one. This is a progress rule, not an arbitrary maximum number of
contacts per frame.

After the first retry, a separate native guard ends the fall when proposed total
horizontal travel exceeds both the original horizontal distance plus 2^-20 and
the original absolute vertical distance times `0x00A37F84` (approximately
1.186666). Repeated contact requests an analytic-anchor reset at `0x009881D0`.
The instrumented executable's `0x005EEB70`, called after rebasing, is a bare RET;
the implementation does not invent an additional callback there.

## Clock and live/trial policy

The unsigned millisecond inputs retain their exact integer values through the
native seconds conversion. Contact-time roots remain wider through progress
comparisons and addition before the accumulated clock is stored as f32. Clear
travel consumes the exact difference between the total interval and that
accumulated float. Landing reloads the accumulated float; a ceiling retains the
wider sum until its returned milliseconds are calculated.

The return conversion follows the original FSTP-float then FISTP-signed-integer
sequence, with nearest/even rounding and the x87 indefinite integer on overflow.
Its raw `u32` image is preserved, and ordinary fall-clock addition wraps. Ceiling
resets bypass that addition. This preserves high-bit and wraparound clock cases
rather than relying on Rust's saturating float-to-unsigned cast.

Live falls promote to FALLING_FAR when ordinary falling with zero launch exceeds
499 milliseconds, or when nonzero launch reaches the native height threshold
at `0x00A1EA9C` (approximately 1/9 unit below launch). Slow fall suppresses this
promotion. Existing FALLING_FAR and ceiling resets retain their phase.

With admitted forward/back/strafe input and a still-nonzero launch, live
updates restore the interval's original horizontal speed and both launch bases
after response. Trial updates suppress this restoration, far-fall promotion,
and live ceiling/resource notification requests. They still return position,
clock, phase, and anchor changes needed by the trial owner.

The step trial's native save/restore pair (`0x0075B480`/`0x0075B4F0`) restores
anchor, elapsed/fall clocks, bases, flags, and horizontal speed. It does **not**
restore current XYZ or the launch-height/downward-speed fields. The
[ground interval's step-trial owner](movement-ground-integration.md) now applies
that partial restoration; restoring an entire world transform would lose the
native trial displacement.

## Native validation

`movement-fall-advance-native.txt` contains 3,474 complete calls to the original
`0x007612B0` and its native callees in the fingerprinted build-12340 executable
(SHA-256 `aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`).
Capture used Unicorn 2.1.4 with x87 control word `0x037F`, explicit unit fields,
and the existing cached-candidate branch. No native function was stubbed.

Cases cover the preceding 807 contact inputs under trial, live, and live
translation policies; high-bit/wrapping clocks; both terminal modes and existing
far-fall phases; exact height gates; and 303 successive native frame samples
across ceiling, wall, floor, and corner scenes. The fixture contains scalar
inputs and outputs only. Normal tests need no executable or emulator.

Across these calls, native observation recorded 4,407 contact iterations,
54 retries with horizontal motion cleared, 84 ceiling resets, 458 far-fall
promotions, and 549 end-fall calls. Notification comparisons observe actual
entries into `0x006E9270` and `0x006EC7B0`; their ordinary subject gates still
execute, including the non-selected-object gate used by the capture.

Rust tests compare consumed milliseconds, fall clocks, active/landed phase,
far-fall state, anchor requests, and notification requests exactly. They compare
fall-owned vectors/speeds within 0.0001 world units and launch-speed float bits
exactly. Ground direction/speed rebuilt after native end-fall belong to the
ground continuation owner and are not claimed as implemented by these tests.

The interval is ready for the local movement owner. It does not yet update an
ECS living transform or send a movement packet from keyboard input. Runtime
`collect_movement_interval` now generates candidates over the native expanded
fall region and rejects incomplete residency. An archive-backed integration
test feeds those candidates into this interval and verifies terrain landing
and selected terrain provenance; see [world geometry](movement-world-geometry.md).
