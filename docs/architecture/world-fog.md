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
