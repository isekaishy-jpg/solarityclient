# Liquid presentation

The specification is the locally owned build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`, together
with its exact client archives. The rendering liquid module currently owns
depth lookup coordinates, generated water depth images, resident animated
texture frame selection, terrain liquid meshes, and a Vulkan world-frame draw
path. The runtime streams terrain liquid materials and submits their retained
geometry with the world camera, light sample, and animation clock. WMO liquid
presentation and swimming remain under implementation; the current Testing
package predates terrain liquid submission.

## Terrain residency and scene composition

The CPU terrain generation retains complete `LiquidType.dbc` and
`LiquidMaterial.dbc` inputs and BLP sources before publishing its geometry.
Native `8A1FA0` dispatches by material primary key. `8A2450` requests precisely
30 surface slots for `%d` names, including failed requests; a missing ordinal
keeps the opaque green Texture.cpp failure image from `4B9760`. Static texture
names retain one slot. Weak material caches share live generations without
pinning departed worlds. GPU geometry retires with its owning ADT generation.

On programmable hardware, `780F50` and `7BD8A0` enable the 2-by-2 MCNK path.
`7CF200` visits its members in row-major order and batches equal liquid types.
Each member retains native strip degenerates and receives a translation from
the first member's origin. Runtime culling uses the resulting world bounds.
The shader receives authored water scale/angle or magma scrolling transforms,
the selected resident surface ordinal, and the final camera's view-space light
and fog inputs. Surface sampling follows the shared texture filtering setting;
procedural depth sampling remains linear and clamped. Retired frame slots
replace sampler resources when filtering changes.

The camera's existing liquid query chooses transparent M2 pass two before
water above the surface and pass one before water below it, matching `4F8EA0`.
Glue continues to use its explicit pass-one-first ordering.

Native `7EBFF0` reads LightParams glow from field four, ocean alphas from
fields five/six, and river alphas from seven/eight. Liquid color bands 14/15
are river and 16/17 are ocean. `liquid_environment_oracle.py` executes that
projection followed by all three original depth callbacks. Runtime tests
decode equivalent generated DBC files and compare every resulting pixel,
covering the column offset and bank mapping together.

Runtime archive tests also cover the complete sequence with a failed 17th
ordinal, material reuse and release, four-member batch ordering and strip
boundaries, and hidden Vulkan rendering of the retained terrain generation.

## Depth coordinates and images

`0x0079E3C0` constructs two 256-float tables, selected by
`0x0079B870` through the liquid definition's first integer parameter. The
material row must have shader discriminator zero or two for this lookup.
The river table scales the authored byte through the exact float constants
at `A3FA24`, `A3FAE0`, and `A3FADC`, saturating after the shallow-water range.
The ocean table spans zero through one. `LiquidDepthCoordinates` preserves
the original float input bits and final table stores.

`0x008A2E20` creates three 8-by-64 procedural depth images. Their callbacks
are `0x008A2BF0` for river and ocean and `0x008A2AC0` for WMO water.
`LiquidDepthTexture` accepts the already packed environment colors and alpha
bytes and produces the equivalent RGBA8 image. Stock generates BGRA8; the
channel reorder is explicit at this CPU upload boundary.

The ordinary rows use signed 8.8 accumulators with a 64-row denominator.
The final row is therefore 63/64 of the distance between the endpoints.
River water converts that last color through RGB/HSV functions `982970`,
`984F60`, `985030`, and `9851A0`, scales HSV value by the float constant 0.9,
and writes an opaque alpha. WMO water uses the deep environment color in
its first four columns and white in its remaining four, with one alpha
gradient across all eight columns.

The environment alpha conversion before these callbacks' accumulators has
a float store after multiplying by 255 and then nearest-even integer
rounding. Consumers must retain that store; performing the multiplication
entirely in double precision changes values such as 0.9.

## Animated textures

`0x008A1D60` first establishes sequence residency, then selects a frame
using the unsigned engine clock from `0x0086AE20`. `LiquidTextureTimeline`
owns only the latter, resident boundary. One frame bypasses timing. A zero
period becomes one millisecond. For multiple frames the client computes
`(clock % period) / period * frame_count`, stores the result as a float,
subtracts one half, and performs nearest-even FISTP. Exact boundaries can
retain the preceding frame; replacing this with floor changes behavior.

Texture acquisition, fallback sequence residency, and archive discovery are
outside this timeline API and still require their runtime material owner.

## Recovered draw contracts

`0x008A1FA0` selects water, magma, or procedural-water material constructors
for material IDs one, two, or three. Missing liquid definitions retry with
liquid type one after the stock diagnostic. The programmable water path
selects `psLiquidWater`/`vsLiquidWater`; it has a separate no-specular family.
The exact `Shaders/Pixel/ps_2_0` and `Shaders/Vertex/vs_2_0` BLS files were
decoded through their GXSH version `0x10003` headers and disassembled with
the Windows D3D disassembler. Water has four vertex variants for its light
count and one pixel variant. Magma has one of each.

The regular water pixel shader combines lit depth-texture RGB, animated
surface RGB, and an animated alpha-weighted specular contribution. Its
output alpha comes from the depth texture and vertex alpha. Magma multiplies
its texture RGB by vertex color and outputs alpha one. These recovered
shader contracts now have build-validated SPIR-V modules and an explicit
512-byte `LiquidShaderUniform` ABI. `LiquidFrame` adds prepared strips to the
unified world submission with one explicit transparent-scene insertion ordinal.

The vertex stages retain the original unnormalized transformed normal,
ambient/directional lighting, first three point lights, and specular power six.
The fragment stages explicitly saturate the two interpolated color registers,
as Direct3D 9 does before executing `ps_2_0`. Vertex fog is clamped before
interpolation and applied to RGB after the material calculation, preserving
alpha. This implicit register behavior is described by Microsoft's
[Direct3D 9 shader documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-writing-shaders-9).

Terrain vertex preparation `0x007CE390` writes transformed source positions,
normal `(0,0,1)`, white vertex color, a depth coordinate from the byte table,
and either position-derived surface UVs scaled by float `0.0600000024` or
authored UV words scaled by `3/256`. `0x007CE270` emits row strips with
degenerate restarts around missing cells. Its cell admission is `0x007CE1F0`.
`0x007D4AB0` batches these surfaces into retained vertex/index buffers.

`TerrainLiquidMeshPlan` implements the per-member terrain geometry boundary.
`0x007CDF80` uses a negative `33.333332/8` grid step, with row controlling X
and column controlling Y, and subtracts the MCNK base Z before storing local
positions. Adjacent global chunk coordinates cancel before the x87 float
store, so the step is identical across the map. The factory constructor
`0x007D4850` initializes an identity matrix. `0x007D4AB0` adds each member's
base relative to the first member before transforming vertices and deriving
UVs. Authored UVs are selected only for vertex format one; format three uses
position UVs despite retaining authored UV words. Mesh indices retain every
native triangle-strip degenerate around holes and at row boundaries.

## Vulkan resource and draw lifetime

`upload_liquid_mesh` admits the complete 44-byte PNC0T0T1 vertex stream and
native unsigned-short strip indices. Uploads use the shared graphics queue
with deferred staging retirement. `retire_liquid_meshes` invalidates handles
immediately after queuing a covering fence; storage remains alive until all
earlier frames and transfers have completed. Neither operation waits for GPU
idle on the CPU.

Each world frame slot owns its dynamic liquid uniforms, material descriptors,
and three procedural images. After the slot fence retires, the frame writes
current uniform and image bytes, updates sampled surface-frame descriptors,
then records the three depth-image copies before dynamic rendering begins.
Capacity grows geometrically within the retired slot. Changing light colors
therefore reuses image storage instead of accumulating cached images.

Native `0x008A27C0` retains bit zero of `LiquidMaterial.dbc` field two as the
queue flag; `0x008A20C0` selects the corresponding queue. The exact table gives
water/procedural water flag one and magma flag zero. `0x0079A870` submits
queue zero after terrain/WMO work, before M2. `0x004F8EA0` submits queue one
through `0x0077F020`/`0x00790A80` between the two transparent M2 passes; its
camera-liquid branch changes which pass precedes water. The frame API accepts
that insertion ordinal from scene composition and retains prepared liquid order.

Both material paths explicitly disable culling and enable depth testing.
The programmable water draw `0x008A56A2` reads the graphics device's capability
block through `0x00532AF0`; this is not a camera-submersion field. D3D capability
initialization `0x0068F253` copies `MaxUserClipPlanes` into that field. The
user-clipping path disables water depth writes while retaining alpha blending;
opaque magma retains the world pass's ordinary depth state.

The GPU frame test renders opaque magma behind transparent water across nine
changing frames. It checks every pixel for the selected river/ocean/WMO depth
image and repeated alpha blending, grows descriptor capacity, reuses frame
slots, and checks immediate handle invalidation and replacement on retirement.

## External verification

`tools/ghidra/liquid_material_oracle.py` maps the fingerprinted executable
without running its entry point. Original depth arithmetic runs with texture
creation stubbed at its provider calls. Original resident sequence selection
runs with a controlled engine clock and immutable resident handles. Complete
pixel callbacks run against a controlled light environment; their color
conversion functions execute original instructions.

The fixtures under `crates/rendering/tests/fixtures` contain all 512 native
depth table words, 409 texture-frame cases including ties and unsigned clock
wrap, and 12 complete procedural images. They are generated outputs rather
than expected values computed by the Rust implementation. External tests
compare depth float bits, selected frame ordinals, and every image pixel.

`tools/ghidra/liquid_geometry_oracle.py` executes original `7CDF80`, `7CE390`,
`7CE270`, `7CE1F0`, and `95DA20` using resident accessor stubs for decoded
height/depth/UV storage. The native material table lookup and all geometry,
transforms, and mask traversal execute original instructions. Its 36 cases
cover all four vertex formats, both depth banks and non-water materials,
global chunk indices, member translations, sparse and empty masks, and
offset rectangles. Tests encode those inputs as MH2O, decode them through
the real asset stack, and compare every position/UV bit and strip index.

`tools/ghidra/liquid_shader_oracle.py` executes the six fingerprinted original
BLS files on an offscreen Direct3D 9 device. It creates no window and never
presents or starts the client. `liquid_shader_frames.bin` retains 36 sets of
inputs and their original 16-by-16 RGBA output. Cases cover all shader families,
zero through three point lights, overbright colors, fog, scaled model/view
transforms, rotated texture coordinates, and spatial depth/surface gradients.
The only native projection adjustment aligns D3D9's integer pixel centers with
the production renderer's negative-height Vulkan viewport.

`tools/ghidra/liquid_vulkan_compare.py` renders those same inputs through the
Cargo-generated SPIR-V with an offscreen Vulkan 1.3 device. Every RGBA channel
in all 36 frames agrees within one 8-bit level; the tolerance allows native
shader arithmetic and final UNORM rounding. The Rust fixture test verifies
the uniform serialization consumed by this comparison. GPU comparison requires
the Python `vulkan` package and accepts the fixture path followed by Cargo's
`solarity-rendering-*/out` directory containing the six compiled modules.
