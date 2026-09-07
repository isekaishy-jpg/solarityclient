# Animation-driven transport timing and geometry

The type-11 transport family uses TransportAnimation and TransportRotation rows
selected by GameObject entry. It does not use the type-15 TaxiPathNode route.
This document covers the systems clock and geometry implementation; runtime
construction, model admission, and frame publication remain to be connected.

Evidence is the pinned build-12340 Wow.exe with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
`0x00713820` selects contiguous stored rows using `0x0070C8C0`/`0x0070C930`,
then calls `0x00711F20` before a map-model handle exists.

## Clock ownership

`TransportAnimationClock` implements `0x00710570` through `0x007108C6`.
The period is the last position timestamp. With no position rows, the phase is
the raw client-plus-object clock. With GAMEOBJECT_LEVEL zero, the phase loops
modulo the period. Otherwise LEVEL splits the state-zero interval from the
interval used by every other signed state byte.

Creation captures the state and converts every ushort dynamic progress value,
including FFFF, using the float at A339E0. The product spills once before
round-to-nearest/even integer conversion. Clock subtraction and addition wrap
at 32 bits, including the x87 integer-indefinite result for out-of-range input.

The state callback `0x00710820` receives the GO's previously notified state
(GO+204) and new live state. `0x00711050` suppresses unchanged type-11 packet
notifications; this GO state is distinct from the behavior's interval state.
Before the old interval reaches the float 0.95 threshold, it retains
progress and reverses within the cached interval. Later changes start a new
interval. Repeated notifications can matter while the cached state differs
from the replicated state. `0x007106D0` settles a reversal at its endpoint
through `0x0070DA40`. The frame callback publishes a passenger phase using the
cached state before evaluating the live state's sample.

`tools/ghidra/transport_animation_clock_oracle.py` executes the original clock
instructions. Only the wall-clock getter and unrelated position/quaternion
providers are controlled. The fixture contains 216 traces covering progress,
signed states, wraparound, threshold boundaries, repeated callbacks, and
endpoint settling. It compares both live-state and passenger phases.

## Authored geometry

`TransportAnimationTrack` retains the stored keys and two interval cursors.
`0x0070DAA0` ignores a single position row, returning a zero offset without a
sequence request. Otherwise it selects the current half-open interval, keeps
duplicate timestamps, interpolates positions, and rotates the result by
GAMEOBJECT_PARENTROTATION. Its sequence belongs to the interval's first key.

`0x0070DC10` ignores a single rotation row, retaining the GO's virtual current
quaternion. With multiple rows it calls `0x00982460` for shortest-arc spherical
interpolation and `0x004F4320` to compose the parent quaternion. A next key at
time zero uses the position period for interval selection, but the fraction
denominator still uses the wrapping unsigned difference between the two actual
key times. Changing that denominator changes the original behavior.

`TransportAnimationSample::pose` adds the original world position, packs the
quaternion through the `0x004F43B0` rules, and rebuilds the matrix from its
decoded quaternion. This differs from the independently retained Euler matrix
of the type-15 family. Quaternion packing is shared with that family.

The geometry oracle executes the original interval searches, interpolation,
parent rotation, packing, decoding, and basis construction. Its only virtual
provider supplies the current quaternion for absent/single rotation rows.
The fixture compares 2,520 samples across 140 tracks bit for bit, including
positions, rotations, selected sequences, packed quaternions, and valid pose
matrices. Non-finite or singular matrices are rejected by the existing placement
boundary. A zero position period and uncovered interval are reported as errors
instead of entering the stock division fault or infinite search; no replacement
geometry is generated.

## Remaining runtime boundaries

The constructor must publish its initial pose before template/model admission,
and state callbacks must preserve GO+204 and their receipt clock.
`0x007139E0` also caps the motion interval at 250 ms, samples a previous pose
when necessary, publishes the passenger clock before endpoint mutation, and
applies passenger-dependent motion thresholds. Model sequence selection and
the ordinary GO model's sequence propagation occur through separate calls at
`0x0077FEC0` and `0x0070BB10`. These lifecycle operations are not supplied by
the systems sampler alone.
