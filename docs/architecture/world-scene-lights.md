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
and transparency inheritance. The world callback adds the exterior directional
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

This covers graphics-scene M2 source publication and its model/water consumers.
It does not establish complete interior entity lighting: MOLT/spatial light
registration, baked floor-light sampling and `7C1730`'s per-entity ambient,
diffuse and shadow-plane callbacks still need their own retained world state
and native coverage. World models currently receive the exterior callback
colors at this boundary. Camera fog and sky have separate coverage in
[world fog](world-fog.md) and [world sky](world-sky.md).
