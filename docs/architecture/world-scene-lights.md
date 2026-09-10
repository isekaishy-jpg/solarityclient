# World scene lights

Animated M2 lights publish before receiving models and liquid batches finalize
their lighting. Each source uses the current bone and placement transforms,
visibility track and instance opacity. Light-bearing models still sample when
their geometry is outside the frustum; the later geometry test prevents those
models from entering draw queues merely because they emit light.

`ScenePointLights` ports the original `834C70` scene grid and `81E400`/`834F60`
query: 64 by 64 wrapped cells, a 20-unit cell scale, X-then-Y traversal and a
four-entry nearest-light insertion list. Equal distances insert ahead of an
existing entry, while a full list rejects a new distance equal to the farthest
stored distance. Comparisons preserve the native extended arithmetic before
storing single-precision distances. Spatial admission is the cell rectangle;
there is no additional spherical range cutoff. Frame publication retires lights
that were disabled, removed or not updated.

Directional lights have a different lifetime rule. `8356F0` links them at the
head only when enabled. Updating their colors or direction leaves the list
order unchanged. The runtime retains authored indices and weak instance
identities, so hiding and re-enabling a source moves it to the native position
without conflating replacement instances or moving placement storage.

`831AF0` queries the translation at matrix `F4 + 30 = 124`, with zero radius.
It does not query the authored mesh bounding-sphere center. Attached models
inherit their parent's query through the same cached hierarchy used for pose
and transparency inheritance. The world callback adds the owner's directional
contribution, then `SetupSunlight` produces slot zero. Up to three selected point
lights occupy the remaining slots. The separate base ambient/diffuse terms are
zero in these world uniforms to avoid adding the sun twice.

Each model instance and its particles/ribbons carry an optional scene index.
The Vulkan M2 scene binding is a dynamic uniform descriptor; the seven fixed
Glue/sky banks remain available at offset zero in their existing descriptors.
World instance scenes follow those banks in frame-slot storage. Their geometric
capacity grows independently of draw and bone counts, including frames with
only particles or ribbons. Every packet's scene index is validated before mapped
writes or recording. Materials retain their own independent dynamic offset.

Liquid providers query the world-space center and radius of their retained
batch. `8A38B0` consumes the first three point candidates and multiplies raw
point diffuse colors by the native `1/255` constant, including authored M2 float
colors. It transforms them into the final camera space. Water sums directional
ambient and specular but retains the last directional diffuse and ray; it does
not run M2's merged-sun finalizer. Interior WMO water supplies its private sun
before scene-light collection. The runtime builds liquid packets after all M2
sources have published.

## Interior model lighting

Units and ordinary M2 GameObjects use the existing native spatial-registration
banks to select their interior group and fallback floor face. `7C7FE0` samples
the face's corrected MOCV values in the current WMO transform. Its dominant-axis
barycentric calculation, fixed integer weights, boundary redistribution and
MOHD ambient addition are preserved. Missing faces use root ambient; missing
MOCV preserves the previous light targets. Rendering and floor sampling share
the same decoded vertex-color correction.

`7A0D60` splits the floor color into diffuse/ambient targets with the original
168/96 thresholds. MOPY's daylight flag blends those targets using floor alpha.
Each owner retains `7A1E90`'s ambient and intensity transition state; `7C1730`
supplies its current colors and interior ray, including the alpha blend toward
the raw daylight ray. Spatial samples are reused until the placement transform
or resident WMO generation changes. Attached models inherit this callback along
with their parent's scene-light query.

WMO doodads instead use MODD's 112/96 split and immediate interior lighting.
Any exterior group reference keeps a doodad exterior (`7BF7F0`). MODD color is
lighting data: it does not tint the mesh or make it transparent through alpha.
Static and replicated WMO owners both use the current registered root.

Exterior entities in the current baked-shadow rendering mode also sample
`7A06A0`'s full-resolution terrain shadow bitmap. The decoded MCSH bits are
retained separately from the edge-corrected GPU opacity texture. Native map
bounds, coordinate rounding, tile/chunk selection and bit addressing determine
whether `7A1BC0` targets half directional intensity. Unit registration excludes
selected WMO floors from this terrain rule. The existing retained transition
smooths entry and exit; movement and terrain publication invalidate the sample.

The pinned `7B5D00` initializer clears every placed MOLT light slot; the traced
`7BDE50` path constructs groups rather than populating those slots. This work
does not synthesize runtime point lights from MOLT records. Animated M2 sources
continue to use the independently verified graphics-scene publication path.

## Evidence and remaining work

- `scene_point_light_oracle.py`: 468 original publication/query captures,
  including equal distances, cell boundaries, wrapped aliases and varying radii.
- `scene_liquid_light_oracle.py`: 84 original queries through the liquid constant
  writer, plus 12 directional enable/disable sequences. The fog-enabled provider
  is the only substituted call in the constant captures.
- Rendering GPU coverage: 36 hidden frames spanning mesh, particle and ribbon
  queues, scene counts from 1 to 96, resource growth/reuse and invalid indices.
- Runtime coverage: offscreen bone-animated sources affect two independent
  receivers and disappear when visibility changes; attached receivers inherit
  the parent query; directional re-enable order matches original linked lists.
- `world_model_floor_light_oracle.py`: 768 native interpolation/split captures,
  plus 96 decoded placement cases covering transforms and absent floor faces.
- `world_entity_light_oracle.py`: 432 native transition and callback captures.
- `world_entity_floor_target_oracle.py`: 384 native daylight target blends and
  MODD splits, including alpha endpoints and random packed colors.
- A decoded WDT/WMO/MODD runtime fixture verifies distinct creature/doodad GPU
  uniforms, the retained ambient transition and MODD mesh-color handling.
- `world_entity_terrain_shadow_oracle.py`: 650 native map/chunk/texel and bitmap
  captures. A decoded ADT runtime fixture checks movement into/out of shade,
  stationary transition reuse and world-unload invalidation.

This covers graphics-scene M2 source publication, model/water consumers and
the ordinary entity/doodad color callbacks. It does not establish the separate
`7C10C0` projected shadow plane, dynamic shadow-map modes, or every specialized
map-entity callback. Native visibility passes also choose an ordinary/WMO fog
bank per model (`793270`/`7C1730`); routing those banks through M2 consumers
remains distinct from shared camera fog. WMO surfaces now consume their own
[group-selected bank](world-model-batch-visibility.md#per-group-surface-fog).
Camera fog and sky have separate coverage in
[world fog](world-fog.md) and [world sky](world-sky.md).
