# Liquid presentation

The specification is the locally owned build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`, together
with its exact client archives. The rendering liquid module currently owns
depth lookup coordinates, generated water depth images, resident animated
texture frame selection, terrain and WMO liquid meshes, and a Vulkan world-frame
draw path. The runtime streams liquid materials and submits retained geometry
with the world camera, light sample, animation clock, and current WMO instance
transforms. Underwater environment selection is documented in
[world fog](world-fog.md).

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

The per-model routing uses `7C23F0`'s ordered terrain/WMO registrations and a
probe at the raw render minimum Z. The terrain branch omits floor occlusion;
the WMO branch retains registered-group selection, liquid masks, type-dependent
epsilon, and the first successful surface. `7C10C0` distinguishes no surface,
surface above the entire owner box, and possible intersection. Ordinary unit
and GameObject roots retain this world sample while their placement and resident
generation are unchanged. Attachments inherit the root state. Scenery's distinct
callback retains its default state.

`8350A0` transforms a crossing plane into view space. `821A20` compares each
model's authored center and first-column-scaled radius against it, retaining
equality at both sphere tangencies. The plane distance remains extended through
the comparisons; rounding it to f32 changes immediately adjacent cases.
Intersecting translucent meshes enter both queues with opposite clip planes.
The Vulkan adapter enables `shaderClipDistance` only when supported, and selects
a corresponding embedded vertex module. Other adapters use the stock camera-side
fallback. The material ABI appends its clip plane at byte 304 (320 bytes total).

Ribbons select one liquid side. Particle routing uses the original sphere
classification without the mesh hardware fallback, with authored bit `0x2000`
forcing pass two. Fully opaque effects remain before either transparent queue.
Opaque compatible-material grouping and faded opaque-effect shader behavior
remain separate scene-order/alpha work; this change does not establish those paths.

`model_liquid_oracle.py` captures 972 original `821C8C..821DDA` classification
cases and 16 complete `8350A0` plane preparations. These are classifier and
query-plane evidence, not a full spatial-producer replay. A decoded WMO/M2
runtime fixture verifies above/crossing/below publication, both camera orders,
particle overrides and opaque-effect ordering. Its GPU readback checks that
opposite mesh clips blend each tested pixel exactly once; effect queues are
checked separately from those mesh pixels. The mounted-model regression verifies
that a rider above water inherits its fully submerged mount's state, while a
crossing mount supplies a surface against which the rider classifies its own
sphere. Workspace validation passes 1,311 tests with 23 explicitly ignored,
plus Clippy across all targets and features with warnings denied.

The optimized installed-archive replay at map 1, `(1100, -5500, -20)`, travels
60 units upward and returns. All seven 400-frame phases complete, including
both camera liquid transitions between ocean type 2 and dry state. Captured
underwater, transition and above-water frames were inspected. This is an offline
integration replay with an ordinary player model; it does not replace the
controlled translucent-mesh pixel proof or the combined populated-world stock
comparison. Captured replay timings are not performance evidence.

## Shader selection audit

Native `8A1FA0` dispatches material IDs one, two, and three to water, magma, and
procedural-water factories. Constructors `8A3F70`, `8A4070`, and `8A4190` request
fixed Water, WaterNoSpec, and Magma shader names. `8A3E00` formats ProcWater with
the suffix supplied by `8A1770`; the world initializer `7997D0` supplies an empty
suffix. This selection does not append an exterior-shadow quality variant.

The installed Water, WaterNoSpec, and ProcWater BLS files each contain four
vertex variants and one pixel variant; Magma contains one of each. The original
pixel instructions contain no shadow-map sampler or receiver operation.
ProcWater's six samplers instead cover two cube maps, two depth inputs, and two
animated surface inputs. A liquid shadow receiver is therefore not a confirmed
missing feature. ProcWater itself remains unimplemented: LiquidType 100 selects
material three, which the runtime currently rejects explicitly. Its wave
generation, cube-map inputs, full material constants, and native GPU comparison
are separate work from ordinary water and the model/water clipping boundary.
The installed archive lookup also fails for type 100's `basicReflectionMap.blp`
and first `basicWaterHeightTex_1.blp` references. The entry and shader's existence
do not establish that the reported Durotar scene exercises this family.

Native `7EBFF0` reads LightParams glow from field four, ocean alphas from
fields five/six, and river alphas from seven/eight. Liquid color bands 14/15
are river and 16/17 are ocean. `liquid_environment_oracle.py` executes that
projection followed by all three original depth callbacks. Runtime tests
decode equivalent generated DBC files and compare every resulting pixel,
covering the column offset and bank mapping together.

Runtime archive tests also cover the complete sequence with a failed 17th
ordinal, material reuse and release, four-member batch ordering and strip
boundaries, and hidden Vulkan rendering of the retained terrain generation.

## World-model liquids

`WorldModelLiquidMeshPlan` consumes decoded root/group geometry. `7A7CC0`
visits MLIQ vertices in row-major order, incrementing X and Y by the stored
float `4.1666665`. Ordinary water UVs subtract the liquid corner and use the
float scale `0.24000001`; magma reinterprets the two authored words as signed
coordinates divided by 256. `7A7920` emits row strips with restart degenerates,
omitting low-nibble-15 cells and leaving high-bit-80 cells for `7A7F60`.

The clipped path constructs each marked cell as TL, BL, BR, TR and considers
its group's portal references whose resident neighboring liquid rectangles
overlap the cell. `7D9470` retains the positive signed plane half-space using
stable edge links. The strip iterator alternates between both ends of the first
surviving edge. Positions retain extended interpolation intermediates; attributes
use the stored float fraction. `7A7F00` interpolates the depth byte and `7A7E50`
interpolates unsigned UV words, both with explicit truncation. Magma subsequently
interprets those interpolated words as signed values. These details preserve
native seams, winding, and unusual authored UV wraps.

`7D7310` first resolves the MOGP liquid type. `7BDE50` derives the instance's
interior flag from MOGI, and `793D20` combines that flag with MOGP flags and
LiquidType flag `0x200`. Interior river types in the native 1-through-20 family
remap to type 17. The material shader column selects authored WMO UVs. The
depth lookup still uses the original resolved group type, before that remap.
If the group type is missing, `793D20` diagnoses it and retries material type
one, while the original group's vertex depth bank remains absent.
Interior vertices use MOMT diffuse color and depth column one; exterior vertices
use opaque white and column zero. `7D4F40` supplies interior water with a fixed
white downward directional light and zero ambient/specular terms.

The terrain and GameObject resource owners load these factories before root
publication. Static MODF, global-WMO, and replicated GameObject instances share
renderer-local source meshes. Shader matrices use the current instance
transform. The last departing instance retires its liquid handles,
and complete world retirement includes all remaining WMO liquid factories.

`liquid_wmo_geometry_oracle.py` captures 148 complete native meshes, including
multiple clipping planes, opposite portal sides, absent cells, neighboring
rectangles, both depth banks, and unsigned magma interpolation. Tests encode
the inputs as real WMO files and compare every vertex byte and strip index.
`liquid_wmo_material_oracle.py` captures 224 native type, tint, UV, depth-column,
and lighting choices. Runtime tests also capture every pixel from shared WMO
water before and after a replicated transform change and verify final retirement.

### Group admission and fog

`799310` adds an admitted group's liquid to the native queue only when its
loaded MOGP has flag `0x1000`. The group's first-visit marker deduplicates
repeated portal callbacks. `793D20` resolves the liquid instance, and `8A20C0`
adds it to its material queue once. The queue append `8A1C30` and material
dispatch `8A2240` do not introduce another camera or portal-box test.

Runtime retains the source group index beside each CPU liquid factory and
builds a dense group-to-GPU-batch lookup when the source is uploaded. Frames
iterate the already deduplicated admitted group list and resolve its current
placement. This replaces the previous scan of every liquid in every resident
placement. Groups without `0x1000` do not create factories, and a dry group's
admission cannot submit another group's water. Terrain liquid bounds culling
continues at its separate terrain submission boundary.

Before drawing a WMO group, `7964A0` sends its accumulated indoor fog bit to
the liquid provider's virtual setter `7D4F10`. Factory `7D5120` initializes that
field separately from the interior lighting mode. Callback `7D4F40` chooses
DayNight bank `8C` or `A0` and writes it through `834990`. Runtime uses the same
group flag to select the liquid fog color; the two final banks share range and
exponent. Interior lighting and tint remain independent of this selection.

`world_model_liquid_fog_oracle.py` captures 64 original factory/setter/callback
queries across both lighting modes, four color pairs, two fog ranges, and
repeated bank changes on each provider. Allocation, the DayNight provider,
process-exit registration, and unrelated scene-light append are substituted;
fog arithmetic and private interior light initialization execute original code.
The runtime Vulkan fixture uses two replicated owners and a dry group before
the wet group. It checks 128 fogged frames across both lighting modes and
owners, plus unfogged tint, missing admission, off-camera admitted packets,
movement, stale departed-owner entries, shared mesh ownership, and retirement.
These controlled checks do not establish combined live-world liquid parity or
native sorting within a material queue.

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

Texture acquisition, failed sequence slots, and archive discovery belong to
the runtime material owner rather than this timeline API.

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
