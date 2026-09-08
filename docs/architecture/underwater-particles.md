# Underwater particulate presentation

Evidence uses build-12340 Wow.exe, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
This boundary covers the camera-relative ambient particle pool. Unit-authored
underwater breath and footstep spray are separate effects and remain the next
part of the water slice. General water lighting and sky remain the final stage.

## Pool and update order

Process world-system initialization `0x00404130 -> 0x00780F50` constructs `0x0079E100` with 4,000 XYZ/size
records, a 30-unit cube, base size `1/36`, and the single fixed texture request
`Textures\WaterPoop02.blp`. The constructor consumes 16,000 shared Blizzard
random words for positions/sizes and four more for drift, even on dry land.
Runtime initializes the pool once before Glue and uses the same shared random
stream as its other recovered native consumers. Map changes retain the pool;
they must not consume another constructor's random words. Native world-system
shutdown `0x007837F0` releases it.

`0x0079B8E0` consumes Z, Y, X, then size for every particle. Coordinates use
the random word's low 23 bits as an IEEE mantissa, scaled into the cube. Size
is uniformly distributed between half and one-and-a-half times the base size.
`0x00790920` reseeds on a changed nonzero camera liquid with flag 8, **before**
installing that liquid's `ParticleScale / 36`. The preceding size therefore
applies to the new seed. Leaving water retains the pool and its previous eye.

World update `0x007831A0` advances `0x0079BF40` before draw-time `0x00790920`
selects the new camera liquid. The pool moves by previous-eye minus current-eye
and its drift vector. Each coordinate wraps only once, with strict comparisons
against the half-cube. A camera displacement longer than the cube side reseeds
the pool and clears that displacement. Extended intermediates survive the
comparison; rounding a coordinate before the wrap can change the result.

Movement discriminator 1 sinks at `-0.02 * dt`; discriminator 2 rises at
`0.02 * dt`; other nonzero values produce no drift. Discriminator zero uses
`0x0079BE50` and the four-word drift reset `0x0079BCC0`. Its periodic wave is
`0x006F7A10`'s cubic approximation, **not a sine-library call**. The direction
normalization, phase accumulation, reset after half a period, and intermediate
float stores are preserved. Drift displacement is not additionally multiplied
by elapsed time.

The native `waterParticulates` console command (`0x0077F6B0`) toggles world
render bit `0x02000000`, enabled by default. It is not a CVar. Runtime exposes
the command in the developer console. Disabling it skips simulation, new-pool
selection and drawing, but still updates the camera's selected liquid ID.
Re-enabling with the same ID consequently does not force a reseed.

## Billboard and device state

`0x0077F9D0` calls `0x0079CA70` after the world and M2 effect queues. Positions
already relative to the eye use only the native view rotation, whose forward
axis is positive Z. Admission is strictly `z > 0`, `-z < x < z`, `-z < y < z`,
independent of the actual projection FOV. The packed PCT vertices use white
RGBA, four half-size offsets, and indices `0,1,2,3,2,1`. Drawing stops after
666 complete quads.

The atlas has 25 regions with a `51/256` grid step, arranged in a nonsequential
bank. `LiquidType` field 13 selects one of five eight-entry patterns; it is not
a texture-slot mask. The first particle always uses region 8. Every inspected
particle, including a culled one, advances the next region using its original
pool ordinal. The portable projection keeps the original row-specific sum
order and float stores. The GPU receives native view-space quads and an
explicit positive-Z to renderer-projection conversion.

`0x0079DFF0` requests texture flags `0x203`. Device tables `0x00AD8F40` and
`0x00AD8FE0`, row 3, specify linear min/mag, nearest mip selection, clamp
addressing, and anisotropy one. This differs from surface ripples' no-mip
sampler. GX blend 2 uses source alpha / inverse source alpha for both color
and alpha. Alpha reference is 1/255 with greater-or-equal comparison. Depth
testing remains enabled; depth writes and depth bias are absent.

The draw sets GX state 11 to zero: **lighting is disabled**. Liquid flag 16
controls GX state 12: **fog**, not lighting. D3D state mapping `0x006A4C30`
maps these to LIGHTING (137) and FOGENABLE (28), respectively. Initialization
`0x006A3A60` selects linear vertex fog (FOGVERTEXMODE 140, value 3); range fog
retains its disabled default. The fog calculation uses positive eye depth,
the world fog interval, and packed fog RGB. Water definitions disable fog;
the installed lava definitions enable it. Fog changes RGB without changing
texture alpha.

Vulkan shares PCT pipeline creation with surface ripples while retaining each
effect's shaders, blend state, sampler and frame resources. Each retired world
slot lazily allocates one fixed 666-quad vertex/index bank and one descriptor.
Ordinary frames reuse these objects. The underwater pass executes after all
world/M2 queues and before glow and UI composition.

## Validation

`tools/ghidra/underwater_particle_oracle.py` executes the original constructor,
selection, drift and advance functions with controlled texture/query providers
and recorded random words. Four complete constructors compare all 16,000
records; 192 history steps compare every retained state word and particle bit,
including movement modes, teleport resets, scale changes, dry cameras and
disabled/re-enabled liquid selection.

`tools/ghidra/underwater_particle_draw_oracle.py` executes the original
`0x0079CBEE..0x0079CD81` loop without math hooks. Its 52 cases compare packed
vertices and indices byte for byte across all five patterns, strict boundary
planes, tilted cameras, empty banks and the 666-quad cap.

Hidden Vulkan captures check ordinary alpha blending, packed-color linear fog,
nearest mip selection, depth testing with no bias, absent depth writes, ordering
after surface ripples, changing textures, empty frames, and reuse of every
world slot. Existing surface-liquid and ripple capture regressions run against
the shared pipeline setup as well. These are offscreen tests; they do not
launch the interactive client.
