# Water camera evidence

The source is the locally owned build-12340 image with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
The portable Systems boundaries below are implemented and tested. Runtime
integration is still in progress; the older nine-ray obstruction and 0.05
water clamp do not establish these native behaviors.

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

## Remaining integration

The runtime must use the dedicated `77F330` volume geometry admission, the
water-inclusive primary traces and vertical anchor constraints from `605D60`,
and the final `6061D0` stage. `77F330` is distinct from movement's `77F340`:
it visits terrain/MDDF chunks before placed WMO roots. WMO volume admission
uses `7AE140`'s camera mask (`0x82` after the transient visited bit), not
movement's `0x84` face exclusion. The old renderability-based camera face
predicate is insufficient evidence for this collector.

`606F90` requests pitch changes at surfaced/submerged transitions while
`cameraWaterCollision` is enabled. Both `cameraSurfaceFinalPitch` and
`cameraSubmergeFinalPitch` default to 5 degrees. `cameraDive` defaults to 1,
`cameraSurfacePitch` to 0 and `cameraSubmergePitch` to 18. Their input and
timed-pitch owners remain to be connected and tested before claiming the
runtime water camera complete.
