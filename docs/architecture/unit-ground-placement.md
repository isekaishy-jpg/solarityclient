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
state. Ordinary unit placement uses the final body yaw and the scene
frame duration supplied by `0x004F8D10`; a gap in callbacks does not become a
larger smoothing interval.

Mounted player placement now follows `0x007197D0`'s `GetModel` selection:
the mount receives the shared unit normal/yaw, its virtual model scale, and
its own model flags and primary timer. The rider's timer cannot provide the
mount's sequence alignment weight. All unit animation/yaw updates precede this
placement pass, including mounts stored before their riders. The rider and its
equipment then inherit the tilted attachment matrix. GPU regressions cover
local and remote players, translated/rotated slopes, duplicate draws, and
dismount/remount state; a separate timer regression distinguishes a rider's
alignment override from the selected mount's primary sequence.

An unchanged mount also retains its complete GPU instance when a rider's atlas
or equipment is rebuilt. `0x00717910` gives the mount a separate model slot and
leaves the same pointer untouched. Residency compares the unit animation owner
and mount model identity before retaining that component; new displays, dismounts,
or new unit lifetimes create new components. Publication remains transactional
and parent-first, updating the retained mount's world transform and ground input
before attaching the new rider. The live regression checks local and remote
animation/event phases, particle allocation and ages, ribbon history, random
consumption, and the retained source's lifetime through an actual atlas rebuild.
Size changes also preserve the instance: `0x0071C0E0` reads the current unit
scale for placement without replacing the mount. Residency updates the scale
key and ground/world transforms transactionally while retaining the source,
clocks and emitter histories. The same live regression resizes both players
repeatedly and verifies the new transforms alongside unchanged instance state.

Creature residency now carries the mount selected by `UNIT_FIELD_MOUNTDISPLAYID`
through the same independent model preparation and retention path. Both
`0x0073D5D0` and `0x00717910` operate on the common `Unit_C` mount slot; this
ownership is not player-specific. NPC riders receive the mounted animation input,
inherit attachment zero and its reciprocal display scale, and leave ground
placement to the mount. The GPU regression covers both models drawing, movement,
slope alignment, repeated size changes with live particle/ribbon histories,
display replacement, dismount/remount, and removal/reuse of the NPC GUID.

Mount playback still uses the existing generic model clock. The retained
mount request owner, mounted behavior routing (`0x007385C0`), and model-specific
completion dispatch (`0x0073BFF0`) remain animation integration work. This
placement change does not claim those timer/lifetime gaps are closed.

The rider's ordinary posture transitions preserve the mounted pose, following
`0x00738B34`. In particular, entering stand state 9 cannot replace the saddle
pose with Submerge 201, and leaving it cannot install StandUp 127 or Stand 224.
Death entry still reaches the full-body override at `0x00738CF3`. The regression
covers both submerged transitions, other ordinary postures, unchanged rider
timers/random state, dismount into the latest posture, and mounted death. Stock's
spell-selected rider pose at `0x00724820` remains separate aura-visual work.

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
Mounted player and NPC registrations select the mount box and mount scale independently
of the attached rider transform; see [unit body scale](unit-body-scale.md).

`tools/ghidra/scene_depth_oracle.py` runs the original camera instruction range
and complete insertion routine, including its original intrusive-list helper,
without hooks. The 489 cases cover direction octants, vertical views, all depth
boundaries, and large world coordinates.

The live outdoor bridge now publishes this admission before draw-frustum
rejection, and the next remote movement service consumes it once across all
catch-up intervals. It admits units outside WMO interiors
when the camera has no WMO registration or the primary camera root admits an
exterior portal. Camera and unit registration reuse the resident native
floor/portal queries.

Camera registration now retains both roots returned by `0x007D59B0`. A static
interior remains primary when both banks hit; a transformed-only hit is promoted
and leaves no secondary root. Liquid and fog still use the primary registration.
Scene traversal needs both because `0x0079A870` visits the secondary root before
the primary. Decoded camera fixtures cover both root visit orders and preserve
each root's portal-adjacent group.

The shared scene traversal now reports `0x007AC060`'s ordered group visits and
first exterior-portal encounters across all camera groups in a root. Portal
callbacks still occur at the maximum visited depth; `0x009CE7E0` initializes
that depth to 10 from `0x00ADFE40`. A portal's first encounter consumes its cache
entry even if its displaced projection subsequently rejects it. The 756 native
scene-query fixtures cover order, cycles, independent group-info flags, shared
initial groups, and depth boundaries.

Exterior projection follows `0x007A8F20`: offset the local vertices by the portal
normal times `0.01`, negate that offset for a positive reference side, and run
`0x007A85E0` without the ordinary near-portal full-window shortcut. The first
twelve vertices supply projection, while `0x007A70D0` scans all original vertices
for maximum nonnegative local forward depth. The 1,188 native fixtures compare
all four screen-window coordinates and depth exactly. The runtime now composes
these queries for the primary camera root. Only adjacent group-info flags
masked by `0x10008` reach the exterior bank: the other portal callbacks and the
initial group's `0x40140` full-window rule affect the separate general bank.
An accepted exterior window enables outdoor unit depth lists even when the
camera is inside a WMO. A transformed-root composition test checks both visible
and rejected portal directions, independent adjacent flags, and shared groups.

`WorldSceneCameraFrame` now supplies the native perspective input to that
projection. It reproduces `0x006BFE60`, `0x006BF370`, `0x006BF6D0`, and the
world-eye corner additions in `0x00795400`; 648 complete original frames match
the combined matrix, all eight world corners, and five clipping planes exactly.
The corner path copies XYZ through `0x00982950` without a perspective divide.
For each placed root it also reproduces `0x007A6E00`'s local forward plane,
including the strict short-direction threshold and extended-value plane offset.
All 312 transformed-root fixtures match the stored camera point and plane bits.
The camera handoff retains `0x00607DB8`'s original view direction independently
of the rounded world endpoint. The native fixtures cover both input forms, and
a renderer regression checks that translation leaves the view basis unchanged.

`WorldModelCameraSceneQuery` also retains the complete camera-root group callback
order. After recursive portal visits, `0x007AD1F0` tests each MOGI group carrying
`0x10000` against the full world frustum and invokes its direct callback. The
shared query includes this separate pass, using the placed group-info bounds.
`WorldSceneCameraFrame::intersects_bounds` reproduces `0x009839E0`'s six-plane
test, supporting-corner sign bits and stored `0xBC9F49F4` negative tolerance.
All 1,344 native bounds fixtures match, including near/far edges and large
coordinates. Runtime indoor-unit admission consumes these retained callbacks
from both camera roots. `0x0079A260` excludes secondary-root groups whose MOGI
flags intersect `0x10008`; all primary-root callbacks remain eligible.

Outdoor unit lists require an admitted exterior portal when the camera is
indoors. Indoor unit membership follows `0x007C2A70`: the selected primary
candidate classifies the unit, and both primary root/group banks link it into
scene lists. Fallback floor banks are not additional scene destinations.
`0x00793270` enables ordinary indoor collision before testing model visibility,
so a matching eligible group suffices even when the model lies outside its
screen window. Decoded WMO/BSP runtime tests cover both primary banks, exact
root identity, secondary-root exclusions, and exterior depth admission.

`WorldSceneCameraFrame::frustum_for_window` now supplies the cropped six-plane
bounds frame used by `0x00790AF0/0x00790E20`. All 1,296 paired native captures
match every corner and plane bit, including narrow and off-screen windows.
Coordinates retain native min-Y, min-X, max-Y, max-X order in normalized screen
space, and every recursive crop starts from the full camera corners. Plane
construction preserves the extended cross-product cancellation before float
stores. These window frames remain separate from portal polygon clipping:
`0x007A72A0` reads the fixed full-camera planes at `0x00CDD108`, while scene
bounds use the current frustum stack at `0x00CDB168 + 0xFC * [0x00CD8798]`.
`WorldModelVisibilityQuery::query_outdoor` reproduces `0x007AD350`'s inherited
clip window and outdoor fog bank. Its 420 native `0x007AC060` captures match
group order, window stores, depth and fog exactly. The composed
`query_outdoor_group` adds `0x007B3A10`'s MOGI flag branches and cropped bounds:
`0x10000` makes a direct callback; `8` starts portal recursion. Camera-root
queries now retain the union of all accepted true-exterior windows, using
`0x007905B0/0x0078F2F0`'s bounds and greatest-depth merge.

Runtime admission now connects ordinary MODF roots to the outdoor depth pass
(`0x00792AD0/0x0079A160/0x007B3A10`). Groups masked by MOGI `0x10008` use their
placed MOGI bounds and the shared 64-bin depth formula. All 603 eligible static
WMO captures match the original insertion bucket; the fixture also retains the
other group flags and transformed-list destinations for subsequent work.
Outdoor cameras supply the full window; an indoor camera supplies the primary
root's merged exterior window. Accepted entry groups traverse their portals,
and `0x0079A260` keeps interior unit destinations or any destination owned by
the primary camera root. A decoded two-group WMO regression reaches an indoor
floor registration through its entrance and rejects a reversed camera or a
screen window that excludes that entrance. Unit bounds remain independent of
the interior callback, which precedes the unit's own draw visibility check.

Replicated moving roots set root flag `0x400` (`0x007B64F0`) and use a separate
ordered overlap list. Both ordinary and moving exterior entries first pass
`0x007B6110`'s inclusive overlap with the full-camera envelope at `0x00CD8F44`.
`WorldSceneCameraFrame::enclosing_bounds` reproduces `0x00984930` over all eight
corners; all 648 native envelope captures match every float store.

For outdoor cameras, the runtime now applies `0x00792BD0`'s depth conversion to
the moving entries in root/group order. All 489 native depth conversions match.
An entry beyond the final bin stops conversion of the entire remaining list;
an entry outside the camera envelope never reaches that decision. For indoor
cameras, `0x00799F80` instead requires overlap with an already-visible group or
an accepted true-exterior portal, then tests the full-viewport frustum. All 960
native overlap-gate captures agree. Earlier moving-root portal callbacks add
visible bounds that can admit later roots, so this pass preserves native order.
Its direct `0x00793270` callback enables exterior-registered units too, while
the final ordinary group pass still requires the unit's interior classification.
Decoded WMO regressions cover the whole-list early return, envelope rejection,
exterior-unit callbacks and admission through a preceding moving root.

Special hidden-model flags remain a separate gap. Rendered NPC placement
still needs an in-world check; the Windows
inspection helper was unavailable during the automated validation session.

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
  visible-object set. The runtime wires exterior depth lists and group visits
  through camera roots, outdoor entrances and moving-root overlap passes.
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
