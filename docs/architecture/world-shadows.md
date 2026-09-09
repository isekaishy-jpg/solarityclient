# Primary world shadow map

The world renderer now builds the animated unit shadow map before its main
terrain, WMO, and M2 pass. Player and creature bodies, mounts, and admitted
attachments contribute eligible material batches. Terrain and model receivers
sample that same frame's map. Sky models retain their separate scene state.

## Original-client evidence

The behavioral reference is the pinned build-12340 `Wow.exe` used by the
repository's native instruction oracles.

- `007BB570` adjusts the raw DayNight ray: multiply Z by five, clamp its lower
  bound to -1.2, then normalize. It consumes the raw ray before the separate
  exterior-light normalization.
- `00874210`, `00873FF0`, and `00875D30` define the registered quality default
  of two, shader selection, and 1024/2048 map sizes.
- `00875F80` retains an unsnapped caster center and quantizes the receiver
  center's X/Y coordinates to the map's texel grid. `007BAC10` and `008750B0`
  construct receiver rows, depth bias, Y convention, and half-texel offsets.
- `007BAFD0` forms the light-space caster volume. Its default `shadowCull=1`
  projects the camera frustum's eight corners, intersects their bounds with
  the map, and crops the admission volume's lateral extent. Empty intersections
  retain the full volume. The actual rendering projection remains unchanged.
- `007BB9D0` collects primary unit casters using the registered radius and
  world AABB, followed by `009839E0`'s six-plane test. Ordinary root admission
  accepts radii from 0.25 through 10000. Recursive attachments inherit admission.
- `00834660` rejects hidden geosets, excluded shader/batch/material flags,
  nonzero material layers, and ineligible blend modes. Its combined element
  opacity threshold is 0.55. `0082DA40` uses 128/255 for textured caster coverage;
  equality survives. `007BBC50` disables culling and restores identity UV state.
- `ShadowMap.bls` and `ShadowMapSL.bls` provide rigid, single-bone, and weighted
  silhouette transforms and direct floating depth. The direct backend samples
  a point-filtered, clamped depth texture at explicit LOD zero.
- `Terrain2`/`Terrain3` use five samples and combine dynamic visibility with
  authored terrain visibility using the minimum before the 0.7/0.3 lighting mix.
- `Diffuse_T1` and `MapObjDiffuse_T1` supply camera-space position and normals
  to the model receiver. `Combiners_Opaque` and the seven MapObj pixel families
  use nine samples nearby, five beyond eye depth ten, border fade, and the
  original fourth-power surface-angle adjustment. The seven families share
  the shadow factor; environment emissive terms and surface opacity remain
  outside that factor. Ordinary MapObj has no Composite effect; MapObjU does.

## Ownership and runtime integration

Each Vulkan frame slot retains its own floating-depth color attachment, depth
attachment, sampler, uniform buffer, and descriptor sets. The slot fence must
retire before resizing or writing these resources. Every active map is cleared,
including frames with no casters. A color-write to fragment-read barrier joins
the silhouette pass to the main world pass.

Caster and visible packets share the unit's sampled bone palette. Units outside
the main camera can retain shadow packets, while their invisible particles and
ribbons do not acquire additional updates. Prepared receiver pipelines are
created during resource publication, avoiding first-use shader compilation in
the frame submission path. The runtime reads `extShadowQuality` from the active
CVar owner; zero disables this map and enabled values select its original size.

The offline world replay records `primary_shadow_draws` alongside terrain-detail
counts and frame/streaming timings. Captures remain separate from performance
runs because readback waits change the measured workload.

## Validation and limits

External fixtures execute the original instructions for 24 projection cases,
1344 material-admission cases, 1800 uncropped unit-admission cases, and 3600
camera-cropped admission cases. Tests compare origins near zero and at large
world coordinates. `world_shadow_caster_oracle.py --cropped` explicitly enables
the native camera crop; the original fixture explicitly disables it.

GPU regressions exercise cutout coverage at alpha 127/128, empty-map clearing,
1024/2048 resizing, mirrored casters, vertical and oblique sunlight, bone palette
offsets, and terrain receivers across all three bone classes (144 captures).
The weighted fixture uses opposed transforms that cancel only when the complete
weighted palette is retained. Separate model receiver captures vary world
height and eye depth independently. WMO captures cover every supported ordinary
and unified material family and check unchanged empty-map color and opacity.
Runtime coverage checks offscreen unit admission, palette sharing, and unchanged
invisible-effect clocks.

An installed-data capture at `(1100, -4290, 20)` outside Orgrimmar shows the
player silhouette on terrain throughout a camera orbit. The ordinary fixture
submits 11 body material packets per frame. A separate GTX 1070 replay at
1280 x 720 uses 2400 frames per phase and the established elevated travel route
from `(1100, -4500, 150)` to `(-500, -4500, 150)`. Capture and profiling are
disabled during measurement, and no compiler runs concurrently.

| Same executable | Outbound mean frame | Return mean frame | Shadow packets |
| --- | ---: | ---: | ---: |
| `extShadowQuality=0` | 4.711 ms | 4.750 ms | 0 |
| `extShadowQuality=2` | 4.816 ms | 4.856 ms | 11 |

Both runs retain 24 residency-change frames, 21 admitted tiles, and 21 evicted
tiles in each direction. This single ordered comparison measures approximately
0.105 ms additional mean frame time on this one-player route; it is not a
populated-world GPU benchmark or a worst-case latency guarantee. Whole-tile
publication stalls remain, with changed-frame streaming means of 20.326 ms
outbound and 15.886 ms returning in the enabled run.

This implementation supplies the primary unit map. Higher quality settings'
additional environment maps, cached static casters, cascade transitions, and
hardware comparison sampling are unfinished. Dynamic grass/detail and liquid
receiver variants and the quality-zero projected entity-shadow path remain
separate work. Native exceptional registration flags outside the ordinary typed
unit owners also need dedicated evidence and runtime coverage. These limits
prevent describing the entire stock shadow system as complete.
