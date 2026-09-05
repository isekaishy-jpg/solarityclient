# Native movement trajectories

The systems crate owns analytic fall distance and contact-time queries through
`MovementFallTrajectory`. They are dependencies of collision response and step
viability. They do not yet integrate a living object's transform or consume
keyboard commands.

The [fall interval owner](movement-fall-integration.md) now combines these
curves with repeated collision response, clocks, and phase transitions. It
returns typed state and actions for the local movement owner.

## Fall curve

The implementation follows build-12340 `Movement_C.cpp` in the fingerprinted
`Wow.exe`, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

| Address | Responsibility |
| --- | --- |
| `0x00986F00` | Absolute downward distance with capped terminal speed |
| `0x00987050` | Converts an unsigned millisecond clock before curve evaluation |
| `0x00988220` | Contact time for a stationary launch |
| `0x00988280` | Contact time with nonzero launch and early/late root selection |

The native curve uses downward-positive launch speed and displacement. An upward
jump therefore has a negative launch speed. The owner supplies `Normal` or
`Slow` after resolving the movement effect. Terminal speeds are the exact native
floats at `0x00B2D9E8` and `0x00B2D9EC`: approximately 60.148 and exactly 7.
Positive launch speed is capped at the terminal speed; upward launch retains
its magnitude. Gravity, half gravity, reciprocal gravity, and their doubled
forms use the separately stored native constants rather than recomputing one
from another.

Distance is sampled from the launch clock. It follows the ballistic curve until
terminal speed and then continues linearly. Frame code must not replace this
with accumulated per-frame gravity. Millisecond conversion retains the exact
integer through the multiplication and narrows to a float only when seconds
are passed to the curve, as shown at `0x00987065`--`0x00987087`. Narrowing the
integer first changes results for clocks above 2^24 milliseconds.

The inverse preserves stock's branch behavior:

- A stationary launch uses its sole root regardless of the crossing request;
  negative distance returns zero.
- With nonzero launch, an early-root request clamps a negative root to zero.
  It does not substitute the later crossing.
- The later root includes terminal-speed travel. It can be negative for a
  downward launch queried at a point above its origin.
- A negative quadratic discriminant is clamped to zero. The result is therefore
  an apex-time result, not a claim that an above-apex height is reachable.

The public scalar boundary rejects non-finite inputs and results. It owns no
packet flags, fall-state transitions, origin height, or horizontal launch state.
Those remain the movement owner's responsibility, including the sign conversion
at any protocol boundary.

## Independent validation

The fixture `crates/systems/tests/fixtures/movement-fall-native.txt` records
1,176 calls to the original x86 functions under Unicorn 2.1.4, x87 control word
`0x037F`. The local capture mapped the original PE sections and supplied explicit
stack arguments. A wrapper called each unmodified native function and stored
its x87 return into a float. No trajectory callee was replaced.

Tests compare all output float bits, including signed zero. Coverage combines
both terminal-speed modes, twelve launch values around zero and the terminal
limits, times before and after apex/terminal transitions, both inverse roots,
negative and positive distances, and fourteen millisecond clocks including
2^24 boundaries, the high bit, and `u32::MAX`. The committed fixture contains
scalar inputs and outputs only; ordinary tests need no game installation or
emulator. Separate public behavior tests check two crossings of a jump height,
linear terminal-speed motion, and invalid scalar admission.

## Ground and step response

The ground-response owner at `0x007620F0` consumes body sweeps, then chooses
support, step trials, sliding, or a transition into falling. `0x00761B00` tests
step motion. Its viability helper at `0x007619C0` snapshots movement state,
starts a trial fall, integrates collision-aware falling through `0x007612B0`,
and checks the horizontal travel before restoring the snapshot. The analytic
curve is required for that trial; it does not replace the trial's collisions.
The fall interval and [ground response owner](movement-ground-integration.md)
are now implemented, including the trial's surrounding save/restore and result
admission. Resident geometry and timestamped runtime input remain to be connected.

The internal movement bit `0x04000000` tracks the step state and its anchor at
movement offset `0x88`. It must remain distinct from the same bit position in
serialized movement flags. Step height comes from `0x006E8FC0` and remaining
height from `0x006E9520`. The native player-control predicate `0x00716710`
selects a unit-specific step height and the stricter support slope threshold;
other units use the separate native values. These choices cannot be inferred
from a wire movement snapshot alone.
