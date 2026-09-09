# Terrain ground detail

Ground effects are the small models selected by MCNK texture layers: grass,
flowers, stones, and other detail recorded in `GroundEffectTexture.dbc` and
`GroundEffectDoodad.dbc`. They use the original dedicated `DetailDoodad.cpp`
path, separate from MDDF scenery and animated M2 scene instances.

The asset boundary retains the layer's 8-by-8 two-bit texture selector and the
separate exclusion bitmap at MCNK header offset `0x50`. A terrain asset worker
loads referenced models and their first textures. Model filenames use native
`World/NoDXT/Detail/` prefixing and the shared MDL/MDX-to-M2 cache conversion.
The renderer expands the first SKIN vertex and triangle lookups directly;
ordinary M2 animation and material-batch selection do not participate.

## Original behavior

- `0x007D3390` seeds the Blizzard RNG with global chunk coordinates, selects
  cells before scattering, applies the sixteen-slot weighted distribution,
  and samples the four native terrain fan faces. Holes, exclusion bits, empty
  model slots, and normals below `z = 0.4` reject placements. The effect's zero
  density means eight placements per selected cell.
- `0x007B1B50` expands positions with native rotation and scale. Doodad flag
  one aligns to the terrain; consecutive placements on the same cell face
  reuse the first aligned rotation. Flag two bypasses MCCV tint. Terrain
  color is interpolated and doubled, while the vertex alpha byte stores
  authored MCSH shadow visibility.
- `0x007B31E0` groups instances into at most four texture buckets. Existing
  buckets accept geometry only when both counts remain strictly below
  `min(groundEffectDensity * 64, 4096)`; a full set drops later instances.
- `0x007C3E70` and `0x00790650` select the nearest AABB corner along camera
  forward and project its depth. `0x007D3FE0` admits visible chunks below
  `groundEffectDist`. This is view depth, including camera pitch.
- `0x007984A0` subtracts camera position from each chunk origin before view
  rotation. The vertex shader receives chunk-local geometry. `0x004F9154`
  dispatches detail before the ordinary liquid/M2 queues.
- `0x007B15D0` supplies the range fade from 85% through 100% of the configured
  distance. Unchanged `DetailDoodad.bls` variant zero applies outdoor light,
  terrain tint, authored-shadow darkening, and vertex fog. `0x007B2D30` uses
  source-alpha blending, enabled depth writes, no face culling, and alpha
  reference 128 initialized at `0x00781048`.
- `0x007D9990` selects the first texture with both repeat-addressing bits set
  through `0x00681BE0`. The renderer preserves the shared filtering and base
  mip policy.

The registered density range is 16–256, default 64. Distance accepts 0–140,
default 140. Live density changes retire cached geometry; distance changes
affect admission and shader fading without rebuilding placements.

## Residency and verification

The runtime prepares geometry when a nearby visible chunk first needs it.
Unchanged frames share its immutable mesh and texture identities. Tile
retirement removes runtime ownership; submitted Vulkan frame slots pin
resources until their fences complete. The renderer then releases abandoned
detail buffers, samplers, and descriptors. Asset work uses the existing terrain
worker and complete-ADT publication boundary.

`tools/ghidra/ground_detail_oracle.py` executes original scatter and mesh
instructions with controlled resident providers. External tests compare every
placement and expanded vertex across density, holes, exclusion, slope, color,
and shadow cases. `ground_detail_shader_oracle.py` renders the unchanged BLS
through offscreen Direct3D 9. A Vulkan test compares 120 lighting, tint, shadow,
fog, alpha-cutoff, and distance-fade cases through archive-backed ADT/M2/SKIN/
BLP inputs and the production world pass.

The detail path also receives the [primary unit shadow map](world-shadows.md)
through native variant one's five samples, authored-shadow minimum, and normal
relief. The shadow-cascade variants remain unfinished. The current world renderer
still lacks the original terrain horizon and sphere-occluder rejection.
Whole-ADT publication and first-use detail generation are still
measurable sources of streaming work. The offline replay exposes submitted
detail texture-bucket counts as `ground_detail_draws` beside its frame timings.

An installed-data comparison outside Orgrimmar at `(1100, -4290, 20)` now
shows the grass, small plants, and stones absent from the previous bare-ground
capture. The orbit submits 26–35 texture buckets. These captures establish
visible ground-detail integration; they are excluded from performance results.

The uncaptured 2,400-frame travel replay from `(1100, -4500, 150)` to
`(-500, -4500, 150)` retained the previous 24 residency-change frames and
21 admitted/21 evicted tiles in each direction. Outbound and return means
were 4.676/4.809 ms versus 4.498/4.473 ms before ground detail. Mean streaming
on changed frames was 18.077/16.466 ms versus 19.852/16.452 ms. The elevated
route averages 7.46 detail texture buckets in both travel phases; it is not a
maximum-density vegetation workload. A conservative tile-bound rejection
preserved every detail draw count across all settled/orbit/pointer/travel
frames while reducing unnecessary per-chunk checks.

The maximum return frame was 75.284 ms, with 42.416 ms in UI processing and
no terrain admission or eviction. This run therefore does not establish
improved worst-case frame latency. The additional visual detail has a measured
rendering cost, and the existing tile-publication stalls remain unresolved.
