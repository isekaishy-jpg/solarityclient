# Terrain lighting and texture values

Terrain and WMO diffuse BLP images use UNORM sampling, retaining the stored
color values in the existing UNORM world framebuffer. Their former sRGB
uploads introduced transfer decoding that the world composition did not undo.
The draw validators and WMO fallback image use the same interpretation.

Ordinary terrain diffuse coordinates now follow the native eight repeats per
MCNK. `7C3C60` builds the negative grid unit `-4.1666665077`; `7C3D90` divides
the original constant `-1` by that unit. `7D0050` publishes the negative result
in c18 onward, and the unchanged Terrain vertex shader applies it to swapped
XY after subtracting the retained chunk origin in c23. The previous four-repeat
absolute-world mapping halved visible texture density. This correction changes
texture coordinates, not world geometry, camera projection or alpha-map density.

The [shared file texture policy](world-texture-sampling.md) independently controls
filtering and authored mip selection.

Animated MCLY layers retain the original eight world clocks across tile changes.
`7831A0` advances each direction using the frame interval, spills to float32,
and resets positive components to zero at 64 while negative components continue.
`7D2D70` uses bits 0..2 for direction and 3..5 for the divisor
`[64, 48, 32, 16, 8, 4, 2, 1]`; bit `0x40` enables animation. It computes
`-clock * (1 / float32(0.24000001)) / divisor * 0.125`, and Terrain.bls adds the
swapped XY offset to each layer independently. CPU serialization retains native
rounding, with a shared 64-entry offset table in the per-frame scene block.
The material alpha and shadow coordinates remain stationary.

Terrain layers preserve the WDT MPHD `0x0004` blend selection through decoded
tiles, mesh plans and Vulkan draws. Without that flag, layers use successive
RGBA overlays. With it, the base layer receives `1 - clamp(R + G + B, 0, 1)`
and the other layers contribute their direct MCAL weights. Those weights are
not renormalized when their sum exceeds one. `7D3E10` and `79E5C0` select the
corresponding original Terrain1 shader variants.

MCLY `0x80` selects unlit diffuse color independently for each layer. `7D2D70`
writes the pixel constant mask; Terrain1 variants 16..31 apply diffuse lighting
to ordinary layers before composition and retain the unlit layer's sampled RGB.
Both kinds retain texture alpha for specular and still receive shadows and fog.
The shared 20-byte draw push block carries atlas coordinates, blend mode,
the unlit mask and four packed animation flags in both terrain-only and
combined-world command recording.

MCNR components retain their stored XYZ order. The dependency names its three
stored fields `x, z, y` and exposes a Y-up conversion; using that conversion
swapped world Y and Z. Native `7C4620` instead multiplies each consecutive
signed byte by its stored float reciprocal of 127. The decoder now reproduces
all 145 captured normals exactly, including asymmetric and negative values.

The ambient/directional branch of Terrain.bls lights vertices before
interpolation, clamps the directional dot product and final illumination, and
then multiplies by the vertex color. An absent MCCV supplies exactly 0.5;
authored MCCV supplies byte/255, including values above 127. The pixel shader
doubles this color after texture multiplication. Terrain vertices serialize
the three float color inputs explicitly so absent MCCV remains distinct from
an authored 127 value.

`7B87F0` expands MCSH into binary visibility in both 16-bit and 32-bit material
textures. The native lookup at A4004C is `[255, 0]`; fixed edges copy the
penultimate samples. Solarity stores its inverse, opacity, and the shader
computes `0.7 + 0.3 * visibility`. Previously the decoder supplied 85 for set
bits and the shader independently subtracted that opacity from all lighting.

Terrain specular now retains diffuse BLP alpha through the ordered layer blend.
The vertex stage computes the normalized view/light half vector, raises its
nonnegative dot product with the transformed MCNR normal to power 20, and
multiplies by the world-light specular color. The fragment stage adds this
independent highlight times blended texture alpha and shadow visibility before
fog. MCCV only modulates the diffuse term. The runtime `specular` setting disables
the highlight, including its vertex calculation.

Native `7CFBE0` writes c24..c27 from `8355D0`'s light sample and sets c27.w from
`A3FFF0` (20). `7D0050` uploads the terrain constants and selects the specular
permutation using `CE049D`, derived from the specular setting and shader support.

## Evidence

- `terrain_vertex_oracle.py`: 145 unchanged native vertex-builder outputs.
- `terrain_texture_coordinates_oracle.py`: eight original constant sets covering
  one through four diffuse layers at two chunk origins. Unchanged Terrain and
  Terrain1 bytecode renders a patterned texture at local and Durotar coordinates;
  the Vulkan ADT/BLP test compares all 8,192 pixels within two RGB byte values.
  The former four-repeat mapping fails this comparison (first red sample 150
  instead of the native 100); the native scale and chunk-relative origin pass.
  The full rendering suite, strict Clippy checks and optimized capture build
  pass. Stationary and orbit Durotar captures at 2560 by 1440 show the restored
  texture density with the same 16x filtering, camera, UI and equipped NPCs.
  The conspicuous bright highlights remain a separate lighting investigation.
- `terrain_shadow_texture_oracle.py`: 12 complete native shadow textures,
  including absent input, both texture formats, and edge modes.
- `terrain_animation_oracle.py`: 384 original offset calculations covering every
  direction/speed pair and six successive updates across the native reset.
  Float32 results match bit for bit. Twelve unchanged D3D9 shader frames combine
  four patterned layers, different speeds and directions, and stationary layers.
  Authored ADT/BLP Vulkan draws compare nine RGB samples per frame within two
  byte values. The former renderer fails after the first nonzero update.
- `terrain_lighting_shader_oracle.py`: 72 colored D3D9 outputs from the original
  Terrain.bls/Terrain1.bls bytecode. The Vulkan ADT/BLP integration checks the
  48 binary-shadow cases, including no MCCV, neutral MCCV, bright tints,
  colored illumination, clamping, and reversed light direction. RGB tolerance
  is two byte values for the native D3D9 interpolator/target conversion.
- The existing 44 terrain fog captures remain in the same GPU test.
- `terrain_specular_shader_oracle.py`: 48 original D3D9 frames over a complete
  flat MCNK grid. The Vulkan ADT/BLP test compares nine spatially varying samples
  per frame across zero/partial/full BLP alpha, shadow endpoints, MCCV, two light
  directions, and the enabled/disabled specular permutations. This establishes
  the single-layer shader path; combined real-world appearance still requires
  validation.
- `terrain_material_shader_oracle.py`: 120 original D3D9 frames across one
  through four layers, every per-layer unlit mask, both blend modes, unequal
  BLP alpha, shadow endpoints, and absent/authored MCCV. Authored WDT/ADT/BLP
  Vulkan draws match nine RGB samples per frame within two byte values. The
  former renderer fails the unlit case (red 42 instead of native 67). Weighted
  cases also cover sums below and above one.
- `terrain_perspective_lighting_oracle.py`: 12 original D3D9 frames at two
  camera heights and local/Durotar origins, with three signed-byte normals.
  Original `7CFBE0`, matrix operations and exterior-light accumulation produce
  the constants; the Vulkan ADT/BLP test compares covered interior pixels within
  two RGB byte values. This extends the flat orthographic test to perspective
  interpolation and native constant production. It does not exercise native
  world-palette selection or environment shadow casters.

The controlled Valley of Trials scene isolates the conspicuous white patches
to the specular contribution: disabling glow reduces their amplification;
disabling specular removes them. These are diagnostic profile copies, not a
change to the product defaults. A 02:00 replay retains strong highlights.
The user's saved stock screenshot is a nighttime Orgrimmar-gate scene; the
earlier combined captures used a different Valley of Trials position at noon.
They establish coexistence of the implemented features, not a matched stock
lighting comparison. Native material selection, complete scene inputs and
environment shadow coverage remain investigation and implementation work.

The original colored regression produced RGB `(6, 23, 54)` where the native
shader produced `(46, 92, 138)` with the same explicit inputs. This exposed
errors that a fully green texture and white ambient light could not reveal.

This verifies the ambient/directional and tested specular terrain paths. It does not certify the
entire outdoor lighting system. Terrain point-light permutations,
dynamic shadow maps, other material families, and visual comparisons at the
same camera/time/settings still require their own integration and checks.
