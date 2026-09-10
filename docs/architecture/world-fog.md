# Camera and palette fog

Build 12340's `0x007816F0` selects fog mode before environment composition.
Programmable hardware uses mode one on map IDs at least 530; older maps use
mode zero. The runtime's Vulkan renderer meets that hardware branch. Map ID
530 is a map threshold, independent of the separate far-clip memory policy.

`WorldFogContext` carries the selected map policy and resolved camera far clip.
`0x007ECD80` first clamps each sampled palette's end to ten. In mode one, an
end at least the stored float `1000 / 36` is replaced by the camera far clip.
Its exponent comes from `0x007ECD00`, using the original end and the stored
product of end and start ratio. The reference width is `min(farclip, 700)-200`;
the exponent is 1.5 for wider fog or `1.5 + 5.5*(1-width/reference)` otherwise.
Mode one also clamps negative start ratios to zero. Mode zero retains them.
The float store boundaries and double intermediates match the original x87
results, including signed zero and the degenerate zero-width/200-unit case.

This conversion runs independently for each normal, underwater, precipitation,
and local palette **before** `0x007EC220` weather and `0x007ED4C0` local blending.
The exponent is a blended scalar, not a curve recomputed from the final range.
Direct LiquidType LightParams overrides use the same conversion. Catalog calls
without a camera context retain the original mode-zero sampling API.

`0x007F16F0` clamps the blended end to the camera far clip and multiplies that
clamped end by the retained start ratio. It doubles the final exponent only
for mode one with a nonzero camera-liquid ID. `WorldFogSample` retains this
final color, range and exponent independently of the source light palette.
Terrain, WMO, M2, water and underwater particle range inputs consume that same
frame. Particle families that originally use linear fog retain that shader ABI.

The order around underwater depth is significant: `0x007F3230` darkens the
working horizon color at `D38B8C` before calling `0x007F0530`, whereas the later
`0x007F16F0` scene-fog pass restores the palette color from `D38BF4`. The runtime
therefore prepares scene fog before applying the horizon/ambient/diffuse depth
operation. Scene fog does not inherit the horizon's HSV depth attenuation.

The native MFOG bank conversion at `0x007ED1B0` differs from DBC palettes. It
first computes the absolute start using the camera-clamped end, then raises
the end to at least 30. Mode one computes its exponent from that pair, replaces
the end by the far clip, and clamps the absolute start to zero. The shared fog
context implements this conversion for the spatial MFOG provider.

## Camera-owned interior fog

The root decoder retains both banks of every 48-byte MFOG record directly from
the chunk. The dependency's 40-byte record reader loses the second bank and
misaligns following records. Nonfinite records, partial records and out-of-range
nonzero MOGP fog indices fail during asset admission.

Camera registration shares one resident-root query between fog and underwater
effects. `7D59B0` can retain a second interior group when a portal supplies the
downward-ray hit. Liquid selection and local MFOG indices use the first group;
both groups contribute to exterior-boundary distance. The first group's four
fog indices pass through `7A1150`'s farthest-first heap, including duplicate
indices and equal-distance ordering. Each overlay quantizes packed colors;
the nearest admitted volume supplies the final flags even at partial strength.

`7D77C0` searches up to four group levels, skips the immediate parent and uses
MOGI mask `0x48` to identify exterior neighbors. Distance is measured to the
portal polygon, including nearest edges when its plane projection falls
outside. Only distances strictly below 25 affect the transition. MOGP mask
`0x48` independently determines whether each camera group enables indoor fog.

`7F16F0` blends from the exterior palette by `clamp(distance * float(0.04),0,1)`.
Dry cameras use bank zero. Wet bank eligibility respects LiquidType flags
`0x20`/`0x100` and MFOG flags `0x100`/`0x10`; eligible liquid flag `0x40` forces
the wet bank even when indoor visibility is suppressed. The underwater exponent
multiplier applies after this composition. Normal frames and benchmark frames
consume the same resolved environment.

## Terrain shader

Original `Shaders/Vertex/vs_2_0/Terrain.bls` uses view Z and constant `c12` to
compute `min(pow(max(z*c12.x+c12.y,0),c12.z),1)` into vertex `oFog`.
The terrain scene ABI now carries the view-depth row, range/exponent and fog
color. Its fragment shader applies the interpolated visibility after layer,
lighting and shadow composition. View-space depth works for both perspective
and parallel cameras. Fog is explicitly disabled for callers that do not supply
an environment; world presentation supplies the complete environment every frame.

## Evidence and validation

`world_fog_policy_oracle.py` runs original `7ECD80`, `7ECD00`, `7ED1B0` and
`7F16F0` across 4,312 captured palette, final-camera and MFOG-bank cases.
`world_weather_palette_oracle.py --far-clip 777` adds 392 complete native
weather/local compositions with power fog. Tests compare original floats and
all retained palette fields, including the exponent.

`terrain_fog_shader_oracle.py` executes the unchanged installed BLS shaders
through an offscreen D3D9 target. Its fixture records both shader hashes and 24
constant-depth cases. The hidden Vulkan ADT test checks 44 corresponding
perspective/parallel frames against those pixels, including off-axis samples,
the fog endpoint, and exponents 1, 1.5, 2 and 7. Runtime tests cover map transfers,
underwater entry/restoration, direct parameter overrides and depth attenuation.

`world_model_fog_oracle.py` adds 288 original local-volume selections and 1,008
complete final fog compositions. `world_model_fog_distance_oracle.py` records
2,640 original polygon distances across rectangular, triangular, skewed and
unnormalized planes. `world_model_fog_portal_oracle.py` adds 270 graph queries;
the decoded-asset tests compare 810 identity, translated and scaled placements.
The camera fixture checks both selected groups in 256 original root queries.
Runtime coverage confirms the dry/wet banks replace scene fog while preserving
the independently depth-darkened light palette.

The final environment also retains both model-consumer banks from `7F16F0`.
The ordinary bank keeps the exterior color (or the forced wet color); the indoor
bank receives the portal-distance color blend. Both use the indoor bank's final
range and exponent. The capture now records 2,016 bank outputs, and the runtime
fixture verifies their distinct colors in dry and submerged interiors. WMO
surface presentation now selects these colors from each group's accumulated
portal fog flag; see [the group callback and GPU evidence](world-model-batch-visibility.md#per-group-surface-fog).
WMO liquids also consume each admitted group's bank through their separate
[liquid provider connection](liquid-rendering.md#group-admission-and-fog).
Attached WMO M2 models now use their [native visibility and fog routes](world-model-doodad-visibility.md).
Other M2 owner types still require their separate routing integration.

`WorldModelVisibilityQuery` now reproduces `7AC060`'s ordered traversal from
an initial camera group using supplied projected portal rectangles. It retains
the current bank until a visited MOGP has `0x48`, skips adjacent MOGI `0x10008`
groups, checks authored portal sides, and preserves the native depth cutoff and
screen-window intersection. MOGP and MOGI flags remain independent. Its reusable
work stack preserves repeated visits and authored reference order. A further
1,440 original-code queries verify those rules on decoded graphs, including
cycles and nearly empty intersections.

`WorldModelPortalProjector` implements `7A9090`'s polygon stage and supplies
rectangles directly to that traversal. The near-portal test considers the whole
authored polygon within the strict 0.01 plane-distance threshold. Ordinary
projection transforms at most twelve vertices, clips using the native 0.0001
classification band, subtracts the eye, and divides with the native minimum W.
The five clipping planes are top, bottom, right, left and **far**; the near plane
is deliberately absent. `WorldModelPortalProjectionFrame::from_frustum_corners`
constructs those planes using `983E70`'s corner order and `7912C0`'s cross-product
float stores. Input corners are already in world space and use the original
positive-forward view convention. This API does not construct a stock view or
convert a Vulkan depth projection into native corners.

`world_model_portal_projection_oracle.py` captures 1,122 original polygon
projections and 108 original camera plane sets. It executes the pinned client's
transform, near-portal inclusion, clipping and projection routines; only the
optional occlusion provider is disabled. Camera cases execute `6BF6D0`, apply
`795400`'s eye addition stores, then execute `984240`. Captures cover perspective
and asymmetric orthographic projections, yaw/pitch/roll, large world positions,
near/far boundaries, twelve-vertex truncation and epsilon neighbors. Regression
tests compare every accepted rectangle and all five planes by float bits.

The projector, traversal, exterior-root admission, and camera adapter now feed
[WMO surface and group selection](world-model-batch-visibility.md). WMO surface
and liquid consumers select the admitted group's fog bank. Attached doodads
consume their separate exterior/group routes. Indoor/exterior sky consumers
still need integration.
The bounded captures above do not establish combined live visual/FPS parity.
