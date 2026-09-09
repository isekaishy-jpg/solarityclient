# Unit ground placement

Evidence uses the pinned build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

## Implemented surface query

`MovementCollisionVolume::ground_normal` reproduces `0x0075EE60` and
`0x0075EC10`. It clips the current ordered collision candidates against a
six-plane box: positive X, negative X, positive Y, negative Y, the body top,
and the foot plane. This query uses the rectangular box, rather than the
nine-plane movement sweep volume's sloped foot faces.

Triangles participate only when their authored normal Z is strictly greater
than the float at `0x00A37F64` (`0x3C8EF859`). Each surviving polygon contributes
its authored normal once, regardless of clipped area. Summation stores float
components after each triangle. The reciprocal count and averaged components
remain extended precision until normalized XYZ is stored. No surviving triangle
returns `(0, 0, 1)`.

The implementation reuses the existing `0x0075B710` polygon clipping rules.
The oracle in `tools/ghidra/movement_ground_normal_oracle.py` executes the original
surface-query instructions without hooks. The 377 checked-in cases cover empty
geometry, all six box boundaries, the strict normal threshold, and mixed ordered
candidate sets at large world coordinates. Tests compare all three float bit
patterns exactly.

## Recovered integration rules

The following behavior is researched but is not yet wired into live spline
movement or model placement by the surface-query change:

- `0x006E9E20` bypasses collision for a remote spline owner when unit flags
  at `+0xBC` lack `0x800000`. Otherwise it calls `0x00762E00` using the delta
  between the sampled path point and the retained movement position.
- `0x004F9F70`, installed by object model registration at `0x00743760`, sets or
  clears that unit flag from scene callback flags bit `4`. The scene calls it
  with flags `5` from `0x00793060` and `0x00793270`. The remaining callback pass
  at `0x00793450` sets bit `4` when registered-model classification byte `+0x25`
  is less than `2`. This is a scene traversal classification, not the network's
  visible-object set. Exact live admission still needs its scene owner.
- After native movement, `0x006EAC40` calls `0x006E9470` with the sampled spline
  point. That correction replaces the retained position only when forced or
  squared displacement is at least `9.0` (`0x009E2FF8`). Thus ordinary collision
  corrections below three yards survive the spline update.
- `0x00762E00` calls `0x0075EE60` after its movement loop, except for an active
  spline carrying `0x2000`. When initial candidate collection fails for an
  active spline, `0x0075D3C0` instead stores the raw target and derives the normal
  from travel direction. That branch does not specify ordinary ground normals.
- `0x007197D0` accepts a movement normal only when Z is at least `0x3EB6E48A`.
  It updates the unit's retained presentation normal with
  `target + (previous - target) * base.powf(delta_seconds)`, using the double
  at `0x00A34BB0` (`0x3F5D7DBF40000000`). It does not renormalize the result.
- `0x0082DD80` selects pitch-only alignment for M2 global flags masked to `1`,
  full normal alignment for masked value `3`, and ordinary heading for `0` or
  `2`. Sequence flags masked by `0xE` to exactly `2`, `4`, or `8` can blend that
  selected basis toward a separately constructed full-normal basis before
  scale. The full-normal basis is built from the original heading; this branch
  does not blend toward upright heading. Combinations such as `6` or `10` do
  not enter that sequence override.

The standalone surface tests establish clipping and normal computation. They do
not establish the live scene admission, spline collision continuation, normal
smoothing, sequence blend, or final rendered pose. The reported NPC slope defect
remains open until those paths are connected and verified together.
