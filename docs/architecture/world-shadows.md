# World shadow maps

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
- `DetailDoodad.bls` vertex/pixel variant one uses the same five sample offsets,
  then takes the minimum with authored detail visibility and blends back toward
  it by `(1.2 - abs(dot(normal, light)))^4`, saturated after the second square.
  The interpolated terrain normal is not normalized again. The primary fade
  plane is zero; distance alpha and fog retain their ordinary detail behavior.
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

Ground detail uses paired baked/primary pipelines with the slot's shared receiver
descriptor. Chunk origins are made relative to both the camera and the shadow
origin on the CPU. The vertex shader adds the small local coordinates afterward,
preserving receiver precision at large world coordinates. Both pipeline variants
are prepared before publication; the disabled variant retains the baked shader.

The offline world replay records `primary_shadow_draws` and each environment
map's submitted packet count alongside terrain-detail counts and frame/streaming timings. Captures remain separate from performance
runs because readback waits change the measured workload.

## Environment rendering backend

`WorldEnvironmentShadowState` and `WorldEnvironmentShadowFrame` expose the three
additional map passes to the renderer. The runtime constructs their admission
volumes before preparing WMO groups and M2 poses, then attaches the eligible
unit, scenery, and WMO packets to the world submission. It publishes the CPU
cache transition only after successful presentation.

The collector distinguishes units, terrain/WMO doodads, standalone game objects,
and moving WMO roots. M2 bone flags `0x2F8` select the animated scenery bank;
moving WMO doodads inherit their owner's animated bank. Static cached scenery
can skip bounds work on frames with no environment updates. Original `007BABC0`
uses the scenery fade-start square, so ordinary scenery stops casting before
its visible fade band. Unit and standalone game-object registrations retain
their independent radius limits. Attachments inherit root map membership and
reuse the same sampled palette across visible and shadow packets.

WMO collection visits resident exterior groups (`MOGP & 0x48`) independently of
portal visibility. Their MODR membership admits attached doodads, and moving
roots select the animated geometry bank. Per-map WMO queues retain the native
2048-group limit.

Original `00874890` refreshes the 40/160/640-unit environment extents. Qualities
three and four retain pairs of textures and publish only after nine, nine, or
twenty-five regions finish. Refresh starts after squared movement exceeds
4/16/1024 and keeps its pending center fixed through the cycle. Quality five
updates all three maps every frame, snapping centers to 2/4/16-unit grids.
`00681F60` and `006A99E0` define the normalized clamp and pixel viewport rounding;
the implementation retains those results even at partial-map boundaries.

`008753F0` zeroes every collector flag at frame setup. Consequently `007BAFD0`
applies camera culling to each update region before `00874890` assigns its
static or cascaded caster mask. The four region bounds multiply the camera
footprint independently, including asymmetric partial regions. Camera culling
changes only the admission planes at scene offset `+624`; the original world
box and full-volume planes at `+6C` remain intact. `00983A60` tests complete
containment against the reversed full-volume planes with tolerance
0.019444443. Cached maps exclude the preceding full pending volume only when
that preceding map updates in the same frame. Cascaded maps exclude the full
primary volume first, then the preceding full environment volume.

`00875C10` updates the light ray without invalidating cached maps. The native
invalidation flag is set when `007831A0` detects disjoint old/new terrain
coverage, or by `007BD9F0` after its world-residency refresh. The runtime resets
the cache when no retained ADT generation overlaps the new set, or when the
map owner or quality changes. An explicit synchronous refresh without generation
replacement has no corresponding runtime operation yet.

Environment color images belong to the world renderer across swapchain slots.
An ordered graphics queue and explicit image barriers serialize partial writes
against earlier receiver reads. New owners clear all textures to visibility one,
matching `00875760`. A region clears only its render area. Its receiver image and
center change together when the refresh completes. Callers must commit the CPU
state only after successful frame submission.

WMO caster packets include the complete MOBA table, including batches omitted
by the ordinary surface callback. Specular secondary passes do not duplicate
silhouettes. `007D82E0` merges entirely blend-zero groups into the minimum-to-maximum
index span, including gaps between batches; other groups retain separate batches.
`007AB760` applies alpha reference
224/255 only to AlphaKey materials; equality survives. Other WMO blends cast
opaque silhouettes. M2 casters retain their existing material admission,
128/255 coverage, and shared animation palette. Packet membership also admits
scenery into the primary map where required by the original quality policy.

The terrain, M2, WMO, and ground-detail receivers consume all four maps without
compiling shaders on the frame path. Original Terrain3 PS8 takes the minimum
of its faded primary result and the first containing environment map. PS16
instead selects an unfaded nine-sample primary result whenever its primary
footprint contains the receiver. Both omit baked MCSH. Original MapObj,
Combiners, and DetailDoodad environment modes take the primary/environment
minimum and apply normal relief; mode three uses an unfaded five-sample primary
result. The final environment map fades at its outer border. Primary-only
terrain and detail modes continue to combine with their authored shadows.

Native fixtures cover 576 refresh/viewport cases, 72 environment projections,
and 1728 unchanged Terrain3/MapObjDiffuse/DetailDoodad receiver executions.
Another 18,816 native queries cover full and partial environment volumes,
camera cropping, and full-box containment at small and large world coordinates.
Vulkan regressions add 240 cached M2-caster frames and 216 WMO-caster frames,
covering publication delay, same-quality owner replacement, quality transitions,
empty caches, primary scenery, and the 223/224 WMO alpha boundary.

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
Runtime coverage checks offscreen unit and scenery admission, palette sharing,
unchanged invisible-effect clocks, caster-class radius limits, full nearer-volume
exclusions, and WMO collection without visible portal surfaces. A further 1050
queries execute the original fade-start admission at each size-class boundary.

The ground-detail oracle captures 540 unchanged shader cases covering depth
equality, the five sample offsets, map borders, authored shadows, and normal
relief. Sixty-four production Vulkan captures compare occupied, empty, disabled, and
out-of-range maps at both primary map sizes, two light angles, and large world
origins. The existing 120 baked detail/fog/alpha captures remain part of the
same regression.

A separate 2,400-frame-per-phase comparison at `(1100, -4290, 20)`, 1280 x 720
on the GTX 1070, measured these whole-frame means before and after adding the
detail receiver. Profiling and capture were disabled, and no compiler ran.

| Ground-level phase | Baked detail only | Primary detail receiver |
| --- | ---: | ---: |
| Stationary | 6.360 ms | 6.456 ms |
| Orbit | 5.667 ms | 5.448 ms |
| Pointer | 6.729 ms | 6.771 ms |

Both runs submitted identical detail counts (28 stationary, 25–36 during the
orbit) and 11 shadow packets per frame. These mixed single-pair changes do not
isolate the receiver's exact cost or establish a general performance improvement.
The new pointer-phase maximum was 41.902 ms; isolated stalls remain.

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

The environment path still needs installed-world validation, including cascade
admission and streaming ownership. Hardware
comparison sampling, liquid receiver variants, and the
quality-zero projected entity-shadow path remain
separate work. Native exceptional registration flags outside the ordinary typed
unit owners also need dedicated evidence and runtime coverage. These limits
prevent describing the entire stock shadow system as complete.
