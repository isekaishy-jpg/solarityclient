# Animation-driven transport timing and geometry

The type-11 transport family uses TransportAnimation and TransportRotation rows
selected by GameObject entry. It does not use the type-15 TaxiPathNode route.
The runtime constructs and samples this behavior immediately during creation,
then advances it before collision registration and local/remote movement.
Packet admission exposes the immediate native constructor boundary, and
the typed presentation retains GAMEOBJECT_PARENTROTATION words 10..=13 across
sparse updates. The raw quaternion bits default to zero, as the field table
does; projection does not synthesize or normalize a rotation.

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
interval. Direct repeated behavior callbacks can matter while the cached state
differs from the replicated state; repeated packet state words are still filtered
by GO+204. `0x007106D0` settles a reversal at its endpoint
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

## Runtime lifecycle and frame publication

`GameObjectTransportBehavior` owns either an animation or a type-15 route.
The creation callback publishes the initial type-11 matrix before later raw
blocks, template replies, or model loading. The coordinator forwards the packet
receipt timestamp through both setup and live dispatch. State changes use that
receipt's client-plus-object clock; progress and flags do not reset this behavior.

`0x006F1490` computes one signed-positive global movement delta, then dispatches
every transport through `0x0074B6E0` before updating units. The runtime supplies
that shared delta to both families; type-15 admission still explicitly samples
with zero elapsed time. An unchanged clock does not move collision references.

`0x007139E0` presamples the last 250 ms after a longer frame. Non-looping tracks
clamp the presample against the anchor with a signed wrapping comparison. The
presample updates cached translation and the packed local quaternion; the final
sample publishes the complete matrix. With linked passengers, translation only
changes at a distance of at least 2^-20. Local and remote owners report admitted
parent lifetimes; unresolved wire GUIDs do not count. Resolved GameObject children
also occupy the list (`0x00712F30`/`0x004F4230` -> `0x0074B340` -> `0x00712EB0`
-> `0x00711AB0`), including those without the unit collision-push flag.

Actual movement parents are independent of GAMEOBJECT_PARENTROTATION. The
animation base query ignores only its own animated translation and retains a
parent's current matrix. `0x004F45B0` composes the current packed local quaternion
with the actual parent's virtual rotation. The animated matrix is already in
world space, while rotation/facing metadata still performs that composition.

Type-11 map admission (`0x0070B5C0`) publishes the current full animated matrix.
Sampling continues without a handle. Sequence keys only change the retained
0x1FA sentinel while a handle exists; all key requests from a long frame reach
the M2 timer in order. Model recreation resets that sentinel (`0x0070B630`).
The first ready-model observation with both DBC arrays empty writes the existing
matrix and skips exactly one clock publication, as in `0x007139E0`.

Runtime regressions cover encrypted create-plus-state packets, early reversal,
duplicate state filtering, pre-template motion, late full-matrix admission,
empty-track readiness, passenger thresholds/removal, long-frame presampling,
and both animation requests' CRT consumption. Placement tests also exercise
animated parents and children against the native quaternion/placement fixtures.

## Remaining boundaries

The separate ordinary GO model's `0x0070BB10` sequence propagation and
`0x0070C550`/`0x006F3910` speed callback are not owned yet for either transport
family. Current movement-length calculation follows the no-ordinary-model path;
the callback's additional float spills require that owner. The `0x00760720`
passenger collision-push kernel, gated by the map handle's `0x00780190` flag,
also remains unimplemented. Existing deck-relative movement and collision
registration do not supply that swept passenger push. The type-11 map-handle
flag set by `0x0077FE00` needs its remaining consumers identified separately.
