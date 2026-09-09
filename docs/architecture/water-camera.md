# Water camera evidence

The source is the locally owned build-12340 image with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
The Systems boundaries below are integrated into the runtime camera. They
replace the previous nine-ray obstruction and fixed 0.05-unit water clamp.

## Subject state and final eye

`6049C0` clears the prior surface/submerged bits, reads the subject's registered
surface through `77F1E0`, and compares surface-minus-origin against unit height
minus two ninths. Equality selects surfaced. The comparison uses extended
precision before the caller stores the float depth. Absence of a registered
surface clears both bits; this is not the elevated camera pivot's liquid query.

`6061D0` runs after primary obstruction. A positive resolved orbit distance
traces from eye Z + two ninths to eye Z - two ninths with mask `0x20000`.
This final trace runs independently of `cameraWaterCollision`. A water contact
puts the eye two ninths on its original side, with exact equality choosing
below. Only a water hit invokes `6059E0` again with solid mask `0x100171`.
When that volume is obstructed, the original camera forward and the newly
resolved distance reconstruct the eye from the pivot. This post-correction
does not feed the changed distance into the persistent zoom lane.

`camera_water_oracle.py` executes the original classification/accessor and
eye-correction functions with controlled scene and object accessors. Its 672
cases check exact float outputs and ordered scene-query arguments, including
missing surfaces, prior state, interface equality, nonpositive distance and
solid retreat. Geometry providers and pitch transitions are separate evidence.

## Swept camera volume

`6059E0` uses the supplied eye-to-pivot direction and a span of distance minus
the 0.2 near plane. It builds a perpendicular basis with world X and a world Y
fallback. The two passes have distinct geometry: expansion 1.0 receives
`mask & 0x30000`, and expansion 1.75 receives `mask & ~0x30000`.

`5FF670` gets the near-plane corners through `6BF6D0`, expands them about their
center, and extrudes them toward the pivot. `791640` transforms the resulting
box to a unit cube. `791380` clips entire scene triangles against its six
planes; `6057B0` retains the greatest surviving cube Z. The final distance is
`distance - greatestZ * span`, clamped at zero. A surviving face at cube Z zero
still reports an obstruction. Nine rays cannot reproduce this volume test.

The collision projection uses `6BFE00`'s diagonal FOV adjustment. The rendered
view's `607D39` call instead multiplies FOV by its fixed 0.6 factor before
`6BF370`; the two projections must not be conflated.

`camera_volume_oracle.py` runs the complete native volume math, including CRT
math, matrix construction, triangle-to-cube transformation and clipping. Only
the FOV accessor and `77F330` scene collection are controlled. All 582 cases
agree on hit/miss and query ordering. The initial 320 axis-aligned cases have
bit-identical corners and distances; rotated cases allow 0.00002 world units
for the remaining extended-precision differences. Tiny distances and thin,
large, partly clipped, water-only and solid-only triangles are included.

## Primary obstruction and scene admission

The runtime uses dedicated `77F330` volume geometry admission, the
water-inclusive primary traces and vertical anchor constraints from `605D60`,
and the final `6061D0` stage. `77F330` is distinct from movement's `77F340`:
it visits terrain/MDDF chunks before placed WMO roots. WMO volume admission
uses `7AE140`'s camera mask (`0x82` after the transient visited bit), not
movement's `0x84` face exclusion. The old renderability-based camera face
predicate is insufficient for this collector. Solid WMO volumes transform the
eight oriented corners into root-local space before BSP selection. Liquid
volumes use those same local bounds and exclude group flag 0x80, without
movement's additional 0x400000 exclusion. MLIQ ray admission separately
requires group flag 0x1000; volume admission does not.

The primary vertical anchor uses the subject's surface/submerged registration,
the unit-height minimum, and the separate mounted obstruction offset. Its
center ray precedes the swept volume and one-ninth retreat. The final interface
stage retains the camera's forward direction. First person keeps an exact zero
distance and therefore does not run the final interface trace.

`camera_primary_oracle.py` captures 508 complete native primary calculations;
the public player-controller regression replays those within its supported
subject-height range (at least 5/6 of a world unit).
Dedicated water-segment, WMO clipping/grid, and terrain-grid captures cover
526, 526, 436, and 636 cases respectively. Water rays retain native mesh
coordinates, cell traversal, triangle order and strict fraction comparison.
Solid WMO rays use native BSP traversal, MOPY admission, and an inclusive
distance limit; converting the accepted distance back to a fraction can round
below that limit. `camera_wmo_solid_oracle.py` covers all 256 face-byte values
with and without cached leaves, plus varied geometry and endpoint cases, for
768 comparisons.

Primary distance and local anchor height feed the persistent camera state
before final eye correction. Collision feedback retains the requested height,
adds the original one-ninth-plus-epsilon cushion, and restarts a two-second
cosine recovery. It shares the principal height target with mounted `$CMA`
changes. `camera_height_recovery_oracle.py` captures 36 histories, including
repeated obstruction and wrapping client timestamps, with exact float results.

The pre-collision pose retains its forward vector, scalar distance and local
height independently of world-space eye and target points. `605D60` reads
these native banks directly. Reconstructing them by subtracting rounded world
coordinates introduced a stationary 0.11-unit alternating retreat near ground
contact at close zoom distances. Keeping the original values matches the
native result in the reproduced near-vertical case. Final water correction
also retains this orientation.

`camera_ground_oracle.py` runs the original primary and volume code against
controlled ground geometry. Its 54 probes include the near-clip threshold,
ground-contact zoom band, three pitches and two world origins. The regression
compares resolved distance, height and eye through the public camera API.

## Water pitch and control modes

`606F90` requests pitch changes at surfaced/submerged transitions while
`cameraWaterCollision` is enabled. Both `cameraSurfaceFinalPitch` and
`cameraSubmergeFinalPitch` default to 5 degrees. These transitions now request
the existing timed pitch owner, or assign pitch immediately during free look.
`camera_water_pitch_oracle.py` compares 144 original transition blocks across
liquid states, collision settings, free look and custom/zero final pitches.

`cameraDive` defaults to 1, `cameraSurfacePitch` to 0 and
`cameraSubmergePitch` to 18. The original automatic movement-pitch block also
requires `721F90` to return false. For the ordinary locally controlled subject
it returns true, except in control mode 13; consequently that block is skipped
in the runtime's current ordinary-player mode. The CVars are registered, but
mode 13 and nonlocal camera-subject automatic dive behavior remain outside
the currently implemented control modes. They must use the native movement
pitch owner when those modes are added.
