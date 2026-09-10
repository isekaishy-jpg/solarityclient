# Attached WMO doodad visibility and fog

WMO MODD models now consume native scene admission before their mesh and effect
packets are built. Static MODF owners and replicated WMO lifetimes use the same
MODR reference lookup. A visible WMO surface alone does not admit every attached
model, and an exterior group has a separate model submission path.

## Native paths

`79A160` visits depth-listed exterior groups. After the group's portal traversal,
`7998A0` places its referenced models into the outdoor model lists. It uses the
horizontal camera plane at `CD8F90`, the world sphere center minus its radius,
the stored 0.03 scale and native bucket conversion. The destination cannot
precede the currently visited group bucket. Already linked models are skipped.
`79A790` drains each bucket after its WMO group visits. `7987A0` applies the size
class gate at the bucket's nominal depth and tests the current outdoor window
with `983D20`. This path preserves the model's existing fog flag.

After outdoor traversal, `799F80` submits models from moving-root overlap groups
directly through `799B70`. These calls precede `79A260`'s final group drain.
The latter admits models from the primary camera root or from groups whose MOGI
flags do not contain 0x10008. Each call uses the group's world-space clips and
its fog flag at that point in traversal. Runtime retains the direct calls' clip
counts and fog values before subsequent callbacks can extend the same group.

`799B70` first compares the model's size class with `78FB60`'s minimum class.
The input is `799310`'s stored full-view leading-box depth at `CD8F80`; it is
different from the horizontal outdoor insertion plane. The first accepted
world-sphere test commits the model's fog flag and clears its pending marker.
Later group visits cannot replace that choice in the same frame, even if
`791CB0` subsequently fades the model to zero. Sphere comparisons include
equality and retain extended X/Z/Y products; the negative AABB tolerance does
not apply to spheres.

`7C1730` reads the model's 0x8000 flag and writes either the ordinary DayNight
bank at `8C` or the indoor bank at `A0` through `834990`. Exterior submission
does not clear a retained indoor flag. The runtime stores the flag with the
model lifetime and updates it on a qualifying group submission. Fog colors
reach both M2 mesh materials and each instance's particle scene descriptor.
The shared final range and exponent and material-specific fog modes retain
their existing behavior.

An outdoor traversal starts with outdoor fog, including the rooms it reaches
through portals. Indoor camera traversal starts with indoor fog and loses that
bank when it reaches an exterior group. Interior lighting flags do not choose
the fog bank.

## Runtime and validation

Placement topology caches `(WMO owner, MODD index)` lookups. Each frame prepares
compact spheres and distance classes, visits referenced models in native scene
order, and retains reusable group, clip and bucket storage. Moving attachments
use their current parent transform and the same `791CB0` scenery fade policy.
Static spheres and classes remain cached. Hidden models that publish scene
lights retain that publication; their mesh and effect draws still require
admission. Effect publication after ordinary models does not remap their saved
admission indices.

Portable native fixtures cover:

- 1,296 original `983FB0` clipping-face cases from captured perspective cameras.
- 489 original full-view group depth stores and 80 `78F570`/`78FB60` class
  boundaries, including non-exact environment detail values.
- 32 complete `799B70` group sequences and 160 `7C1730` queries, with empty and
  repeated clips, reversed references, class rejection and frame changes.
- 288 original `7998A0`/`7987A0` exterior queue/submission cases, including
  bucket clamping, out-of-range rejection, class gates and both retained banks.

The admission oracles substitute loaded records, model activation, downstream
submission and the external occlusion provider. They execute the original
class, list, clipping and fog-query code. Optional occlusion is disabled.
Tools live under `tools/ghidra/world_model_doodad_*_oracle.py` and
`world_scene_sphere_oracle.py`; they require the pinned locally owned client
and Unicorn. Runtime tests consume captured outputs without requiring it.

The Vulkan fixture exercises a decoded two-group building with an exterior
attachment, a visible room attachment and a room attachment outside the portal
window. It checks static and replicated owners, outdoor/indoor camera changes,
parent translation/rotation, shared resources, retirement, fully fogged mesh
pixels, particle descriptor colors and hidden effect clocks. Scene collection
tests separately check primary/secondary gates and frozen overlap snapshots.

Testing Build 78 includes the surface, liquid and attached-model consumers.
The optimized offline World harness was rebuilt from the same implementation
and completed 1,260 frames over seven phases at the Orgrimmar gate, including
camera orbit and a 200-unit outward/return travel segment. Captures were inspected;
the run reported no runtime or Vulkan errors. This checks the combined installed
asset path, not populated-server behavior, interior travel or stock visual parity.
Captured-frame timings are not performance evidence.

This implements the attached-model consumer. Native terrain-horizon and sphere
occlusion, specialized particle/ribbon behavior, other entity fog consumers,
and combined live-world acceptance remain separate work. No FPS or complete
world-visibility parity follows from these controlled fixtures.
