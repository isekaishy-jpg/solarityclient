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

## Implemented model pose

`M2GroundNormal` starts upright and reproduces the accepted-normal smoothing
slice of `0x007197D0`. It retains the unit's normal independently of model
replacement. Its model transform reproduces `0x0082DD80`: heading construction,
pitch-only or full-normal alignment, optional sequence blending, and final
scale. The full-alignment side vector is normalized, while the retained normal
and derived forward vector are not independently normalized. Matrix blending
stores both scaled matrices before addition, including their translation.

`M2ModelSequenceTimer::ground_alignment_weight` uses the unwrapped primary
timer sampled by `0x008266B0`, not the bone pose's modulo time. Both the timer's
initial elapsed product and `0x00407930`'s speed division truncate through an
int64 conversion before retaining the low word. The half-duration division
remains extended precision through the optional flag-4 inversion. Zero-speed,
negative-speed, zero-span, and wrapping-clock cases follow the executable.

`tools/ghidra/unit_ground_pose_oracle.py` executes the original smoothing slice
and full model-placement function without instruction hooks. The latter runs
the original sequence lookup, primary timer query, basis construction, blend,
and scale against controlled model records. Tests match 216 smoothing cases
and 520 complete model matrices and sequence weights by exact float bits.

The runtime publishes local and remote collision normals through the unit's
lifetime state. Model loading, replacement, and duplicate draws preserve that
state. Ordinary unmounted unit placement uses the final body yaw and the scene
frame duration supplied by `0x004F8D10`; a gap in callbacks does not become a
larger smoothing interval.

## Recovered integration rules

### Outdoor scene depth admission

`WorldSceneDepthFrame` implements the camera plane from `0x00795400`, the
leading raw registered AABB corner from `0x00790650`, and outdoor M2 insertion
from `0x00792E60`. The XY plane retains the full-forward intermediate values
before horizontal normalization. Positive depths are multiplied by the float
at `0x00A3F7EC`, stored to float, reduced by `0.5`, then rounded to nearest even.
Nonpositive depth enters bucket zero; an index at least 64 is unvisited.

`0x007B5590` classifies every eligible registered M2 into these 64 lists.
`0x0079A790` visits every list, and `0x00793060` changes classification to 1
before frustum or occlusion rejection. Therefore outdoor movement admission
does not mean that the unit was drawn. The raw unit registration transform
from `0x007370D0` supplies the bounds, before the smoothed model tilt.

`tools/ghidra/scene_depth_oracle.py` runs the original camera instruction range
and complete insertion routine, including its original intrusive-list helper,
without hooks. The 489 cases cover direction octants, vertical views, all depth
boundaries, and large world coordinates.

The live outdoor bridge now publishes this admission before draw-frustum
rejection, and the next remote movement service consumes it once across all
catch-up intervals. It admits ordinary unmounted units outside WMO interiors
when the camera has no WMO registration. Camera and unit registration reuse the
resident native floor/portal queries.

Camera registration now retains both roots returned by `0x007D59B0`. A static
interior remains primary when both banks hit; a transformed-only hit is promoted
and leaves no secondary root. Liquid and fog still use the primary registration.
Scene traversal needs both because `0x0079A870` visits the secondary root before
the primary. Decoded camera fixtures cover both root visit orders and preserve
each root's portal-adjacent group.

An interior camera requires an admitted exterior portal window. WMO-bound units
instead depend on visited group lists through `0x00793270`. That portal/group
scene bridge, mounted model registrations, and special hidden-model registration
flags remain unimplemented; the outdoor bridge is not a complete scene answer.

### Movement and presentation

The shared movement and presentation integration follows these recovered rules:

- `0x006E9E20` bypasses collision for a remote spline owner when unit flags
  at `+0xBC` lack `0x800000`. Otherwise it calls `0x00762E00` using the delta
  between the sampled path point and the retained movement position.
- `0x004F9F70`, installed by object model registration at `0x00743760`, sets or
  clears that unit flag from scene callback flags bit `4`. The scene calls it
  with flags `5` from `0x00793060` and `0x00793270`. The remaining callback pass
  at `0x00793450` sets bit `4` when registered-model classification byte `+0x25`
  is less than `2`. This is a scene traversal classification, not the network's
  visible-object set. The runtime currently wires the exterior depth-list case.
- After native movement, `0x006EAC40` calls `0x006E9470` with the sampled spline
  point. That correction replaces the retained position only when forced or
  squared displacement is at least `9.0` (`0x009E2FF8`). Thus ordinary collision
  corrections below three yards survive the spline update.
- Before collision, `0x006E9C30` has a separate snap-and-return rule when
  squared horizontal speed exceeds `3600.0` (`0x00A32834`) or total squared
  displacement exceeds `9.0`. Unlike the post-collision correction, both
  pre-collision comparisons are strict. The calculations retain extended
  displacement products and the seconds conversion from `0x009E1134`.
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

`MovementIntervalDrive` retains `0x00762E00`'s original direction and speed
through collision substeps. Its 356 unhooked native fixtures compare all float
stores exactly. Runtime walking splines use that drive with the shared ground
and fall solver, preserve corrections below three yards, keep the compact path
summary, and publish the resulting normal. Spatial, parabolic, falling-path,
and hover response owners remain outside this walking integration.

Runtime regression tests cover repeated slope intervals without double motion,
scene-rejected raw placement, pre-collision snap, missing-geometry travel
normals, completion, and admission lifetime across model replacement. A rendered
stock-map check and the remaining scene cases are still required before closing
the reported NPC slope defect.
