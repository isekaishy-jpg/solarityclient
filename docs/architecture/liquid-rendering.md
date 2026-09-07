# Liquid presentation

The specification is the locally owned build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`, together
with its exact client archives. The rendering liquid module currently owns
depth lookup coordinates, generated water depth images, resident animated
texture frame selection, and terrain liquid meshes. Terrain and WMO draw submission and swimming remain
under implementation; these CPU boundaries alone do not display water.

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
shader contracts are not yet a Vulkan draw path.

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
