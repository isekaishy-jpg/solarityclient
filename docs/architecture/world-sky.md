# World sky

The native sky owner builds a reusable dome at `7F2470`: two poles, five
24-vertex latitude rings and 300 indices forming one connected triangle strip.
Its latitude coordinates use the executable's cubic periodic approximation;
longitude uses sine/cosine. A fixed negative cosine offset lowers the sphere
so the narrow lower rings surround the horizon. `9ACB00` draws this geometry
with scale `6.6666665`, opaque vertex colors, additive blending and no depth
writes. `7F09B0` reserves depth range `0.9990234375..1` for the sky.

`7F0530` composes five authored sky colors and the final environment fog color.
The six-key day table controls dawn/dusk highlight strength. `4F8410` supplies
the normalized camera direction; `7F3920` derives its azimuth, using the X/Z
fallback when horizontal squared length is at most `0.0001`. The second
six-key table varies each ring's packed color around that angle. The top
retains the first sky color; the lowest ring and bottom pole use fog color.
Packed interpolation and its half-byte rounding happen at each native blend.
The angle wrap keeps extended precision until the original float stores.

`WorldSkyDome` retains this geometry and updates only its colors.
`WorldSkyFrame` removes camera translation and carries the common projection.
Each retired Vulkan frame slot owns one reusable 3.5 KiB mesh bank. One sky
draw precedes the world geometry queues and does not consume a texture.
The runtime takes continuous sky time and integer band time from the same
server-anchored clock sample, and uses the post-liquid environment palette.

`world_sky_oracle.py` executes the fingerprinted original constructors,
gradient, color conversions and camera-angle instructions. Its only mesh
substitution is resident storage at dynamic-array allocation boundaries.
Four radii establish positions and indices; 836 gradients cover 131 palettes,
highlight branches, day boundaries and angle wrapping, comparing every color
byte. Seventy-three direction captures cover axes, poles and the fallback
threshold. The hidden Vulkan test checks full viewport coverage, camera
translation, rotations, color changes across slot reuse and world occlusion;
an additional gradient frame checks the horizon's fog color and is available
for visual inspection through `SOLARITY_SKY_CAPTURE_RGBA`.

The cloud backend now follows `7F20E0`, `7EFD00`, `7F1010` and `9ACD40`.
Its dome has 177 positions, radial UVs, white colors with horizon alpha falloff,
and one 374-index strip. The procedural texture uses the original CRT noise
table, cosine smoothing, four octaves, grain lookup, approximate reciprocal
square root, color quantization and first-column retention behavior. Eight
rows update per frame; a complete initial refresh and the original two-bank
switch determine the displayed image. Sampler row one uses linear min/mag,
no mip filtering and clamp addressing. Clouds draw after the gradient with
source-alpha/inverse-source-alpha blending and the same reserved depth range.

Each retired Vulkan slot owns its mesh, staging buffer, 128-square BGRA image,
sampler and descriptor. Content changes trigger uploads; the cache remains
dirty until submission succeeds, including acquisition failure before recording.
`WorldCloudLighting::sample` ports the original celestial ray/sphere projection
and cloud color provider. Its final argument is the precipitation attenuation
scalar (`D38B88`), not liquid depth: `4F8410` reads it from the weather owner.
The native daytime sun-selection interval includes both boundaries.

`WorldCelestials` ports `7EECC0`: cyclic polar/azimuth/size tables, cubic
trigonometry, camera-centered radius 12, first-moon size multiplier 1.75, and
the second moon's 1.7-day cycle with native 16-bit calendar-phase rounding.
The native provider must supply both cyclic time and calendar day index.

The cloud oracle compares every pixel across 44 original updates, all noise
and grain tables, geometry and 56 lighting-provider samples. The celestial
oracle executes the entire ephemeris unchanged for 1,250 camera/day/calendar
samples, including neighboring floats at table boundaries. The hidden GPU
cloud test compares 20 captured frames against an independent scalar projection,
bilinear texture sample and blend of the native mesh and pixels. The complete
five-test sky/liquid/ripple/underwater frame group and rendering library checks
pass, along with Clippy for all rendering targets.

Cloud noise prepares the four octaves' Y/Z interpolation weights and permuted
Y/Z bases once per row. Columns in the same X cell reuse its four left values
and f32 corner differences; each pixel still performs the original f64
interpolation in the original order. This scratch state lives for one row,
with no allocation or retained phase cache. The eight-row update budget,
initial/invalidated full refresh, derivative history, lighting, and texture
bank switches retain their native paths. Five additional oracle updates enter
through the original elapsed-time provider and cover phases 65,534, 65,535,
0, and 1, checking every changed pixel across the 16-bit phase wrap.

The active runtime now retains and submits both gradient and procedural cloud
domes. It samples cloud lighting from the native sun/first-moon positions and
the completed environment's cloud bands, including the camera's liquid bank.
The simulation advances once per world frame on the client millisecond clock.
The earlier native cloud static constructor consumes 256 CRT random values;
the client-thread random stream now accounts for this before M2Initialize's
twinkle seed and later Glue/world emitters.

`RealmSkyTime` samples `76CFF0`'s unsigned millisecond progression in extended
precision, with the native minute-to-day multiplier and day wrap. Band time
uses the original float store and nearest-even conversion after subtracting
one half. The exterior ray samples this continuous time, avoiding half-minute
steps. `realm_sky_clock_oracle.py` supplies only the monotonic tick provider;
330 captures compare the complete native clock and normalized exterior ray.
The existing water-light uniform tests continue to pass.

The second-moon calendar provider uses CRT local-calendar midnight divided by
86,400, as in `76C1F0`. The realm supplies the date; the platform supplies its
timezone/DST conversion. `Map.dbc` column 62 retains minute-of-day overrides:
`-1` uses realm time, valid overrides reset the lunar calendar index to zero,
and invalid minute ranges select the original noon fallback. Calendar tests
cover leap-day rollover, while clock captures cover fractional and multi-day
progression. Production startup supplies the complete map override catalog.

Weather lighting now consumes `SMSG_WEATHER` (`0x2F4`, receiver `526530`):
an exact nine-byte weather ID, intensity and instant flag. `WeatherCatalog`
loads all eight authored fields; unknown IDs select native clear weather with
weight one. The installed table contains 32 records, and all 715 installed
light volumes have both precipitation banks. Queued updates retain packet
order and receive times, and world replacement/disconnect clear old weather.

`7846A0` anchors new transitions from the previous target, including interrupted
fades. `784850` separately interpolates precipitation grade, lighting grade
capped at one quarter, and the authored weight. The palette/cloud attenuation
is the product of stored lighting grade and stored weight, multiplied by four
and capped at one. Float stores, unsigned elapsed time, instant changes and
`78D170`'s small-change threshold are retained. The transition oracle runs
248 native setter calls, 672 interpolator calls and 144 complete threshold frames.
Resource type and nonempty texture lookup are controlled inputs; the threshold
frames omit player/camera and particle advancement through explicit boundaries.

`7EC220` blends weather banks two/three into normal banks zero/one before each
global/local palette enters spatial composition. Its RGB opacity is a rounded
byte, with integer arithmetic for intermediate colors and exact replacement
at 255. The sky-highlight flag, skybox/cloud-type IDs and three unrelated sky
scalars retain the normal palette values. The oracle compares 392 palettes,
including ordered local overlays and opacity rounding boundaries; every result
is checked through both exterior and underwater WDBC queries. LiquidType's
direct LightParams override bypasses weather palette selection. Live environment
tests cover weather updates, underwater lighting, direct overrides and reset.

Weather particle rendering, particle drainage during resource-type switches,
and weather ambient audio are not implemented by this lighting owner. Its
current type follows accepted weather updates; particle residency must replace
that input when the precipitation owner is introduced. WMO sky visibility is
connected below; the separate global sky override provider remains unwired.

The three celestial texture requests now use `Textures/sunCenter.blp`,
`Textures/moon.blp` and `Textures/moon02.blp`, as in `9AD0B0`. Their parsed
sources and GPU handles survive world replacement. All three installed assets
decode successfully through the archive stack (128x128, 128x128 and 64x64).
Sampler row one selects linear min/mag, no mip sampling, and clamped edges.

`7EDBE0` constructs six local Y/Z vertices and a four-index strip. `7EDEE0`
clips that strip at the camera-relative horizon, carrying the intersection into
the V coordinate. It inserts two vertices when the strip crosses the 0.4-unit
fade band, then substitutes per-vertex opacity below that band. This opacity
replaces weather alpha. `9ABB60` builds the positive-forward billboard basis,
including its original axis-aligned fallback. `9AC660` adds the body-minus-eye
translation and multiplies by the translation-free view before projection.

`WorldCelestialLighting` retains the native constructor and update behavior:
sun and first moon receive LightIntBand channel nine, while the second moon
retains its zero constructor RGB. Nonzero weather replaces all three alphas;
clear weather refreshes only the first two colors. The process owner keeps
that state across map replacement. This is deliberate native behavior rather
than an inferred bright second-moon tint.

Each retired frame slot owns three six-vertex PCT banks and texture descriptors.
The draws precede the additive gradient and alpha-blended clouds, using native
source-alpha/inverse-source-alpha blending, reserved sky depth and no depth
writes. Ordinary world depth continues to occlude all sky layers. Both gameplay
and benchmark presentation submit the same celestial frames.

`world_celestial_mesh_oracle.py` executes both geometry functions and the basis
without hooks: 1,912 mesh cases cover horizon/fade thresholds, world heights,
sizes and alpha; 267 bases cover axes, poles and the fallback threshold. Packed
colors and topology compare exactly. Geometric x87 rounding ties allow one
final float ULP; every captured basis matches bit-for-bit. The color oracle
executes `9D0760` and the unmodified `7F36EF..7F3809` update block for the
constructor and 48 ordered weather/color changes. These join the existing
1,250 ephemeris cases. The hidden Vulkan test checks 16 textured frames against
a scalar perspective-correct sampler, changing descriptors across slot reuse,
the horizon split, gradient composition and opaque-world occlusion.


The stars now use the original `Environments/Stars/stars.mdl` request, resolved
through the ordinary M2/SKIN/BLP owners. `7EE0D0` samples keys at 0.125, 0.1875,
0.9375 and 1.0, then truncates `254 * value + 1` to a byte. `9ABD50` submits only
above byte one, with opacity scaled by the stored float `1/255`. Weather does
not independently attenuate this curve. The unmodified native oracle covers
4,442 times, including adjacent floats at curve and alpha boundaries.

`WorldSkyModelFrame` adds separate stars and authored-skybox queues around the
existing celestial/gradient/cloud passes. It has a camera-relative M2 scene
uniform; bones and materials share the ordinary frame slot's fence-retired
storage with explicit offsets. The reserved sky depth is applied to the sky
projection. Meshes, shader permutations, textures and ordinary model sequence
playback reuse the existing M2 implementation. Stars retain their scene clock
across map replacement, and skipped daytime intervals are included when the
model next advances. Their independent scene has no environment fog or local
lights (`834900` clears the model environment, and `81FB10` disables fog when
its `+0xB0` field is zero).

A hidden Vulkan test checks twelve frames of separate sky/world uniforms,
changing bone prefixes, stars/gradient/skybox blending, native depth writes and
world occlusion. The real-archive runtime test submits all seven installed
stars materials over ten frames, verifies exact invariance under camera
translation, and checks opacity changes. The installed stars sequence has
static geometry and material tracks; the retained ordinary clock does not
invent motion. Its two textures render the authored star field. The six
existing liquid, ripple, underwater and other sky GPU tests also pass.

The three ordinary LightSkybox slots now resolve through the installed catalog
and retained M2/SKIN/BLP resources. `7F3230` admits positive-weight valid rows in
palette order. A replacement row above the strict float 0.99 cutoff resets the
output count; its authored flag bit two keeps an overlay instead. Empty paths
still participate in that compaction, and the first unused weight is zeroed.
`7F09B0` suppresses stars, celestials, the gradient and clouds only when a loaded
replacement model exceeds that cutoff. Skybox models follow the clouds in
palette order, preserving each model's opaque and sorted transparent batches.

`7F30C0` retains models by case-insensitive path across world replacement.
The first request's flags remain attached to that cached animation owner even
when a later DBC row names the same path with different flags. All resident
skybox models advance before active slots perform `7ECF20` phase checks.
The phase cache reads the current primary's scene span once. Flag bit one
requests Stand at the realm minute's fraction of that span only when the
signed wrapping absolute minute change is at least two. Every sampled minute
replaces the previous minute, so consecutive one-minute steps do not seek.
Speed uses the native stored float reciprocal of 86,400,000 milliseconds;
phase offsets preserve the x87 product's 64-bit significand and truncate.

The realm's whole-minute provider now advances once per gameplay service frame
through `76D900`'s repeated float stores and strict greater-than-one rollover.
It is separate from continuous daylight and Map.dbc daylight overrides. Its
oracle executes the original update, minute increment and getter with only
calendar-day mutation, registered callbacks and monotonic time supplied.
The capture covers fractional frame intervals, minute/day rollover and unsigned
elapsed-clock wrap. Existing continuous sky-clock and calendar tests remain.

Each LightSkybox slot has a fence-retired M2 scene descriptor for its animated
authored directional/point lights; stars keep their independent empty bank.
The installed 124 DBC rows select 48 unique model paths. All models load and
render through the runtime owner; all have hardcoded textures and no particle
or ribbon emitters. PortalWorldLegionSky supplies the installed lit dome.
Archive-backed GPU tests exercise all 48 models, changed bone prefixes, exact
camera-translation invariance, opacity fades, cached flag reuse, overlapping
slots and that dome's authored lighting. `world_skybox_oracle.py` captures the
unaltered selection block and phase helper, including absent/empty paths,
cutoff-adjacent floats and wide unsigned animation spans.

## WMO sky visibility and replacement

The camera scene retains two independent portal banks. Every accepted
`7A8F20` callback reaches `790AB0`'s sky bank; only adjacent MOGI flags masked
by `0x10008` also reach `790AD0`'s outdoor-object bank. `795D40` seeds full sky
when either primary camera group has MOGI flags masked by `0x40140`. `79A870`
clears that seed and both portal banks after a secondary camera root, then
keeps the primary root's portal results. An outdoor camera starts with full
sky. Outdoor group recursion and final direct callbacks do not select MOSB.

Native root loader `7D7470` retains the MOSB string and nulls an empty name.
It also clears MOGI flag `0x40000` when no name remains; the loaded MOGP flags
are independent. Recursive `7AC060` visits with MOGP flag `0x40000` publish
their root's MOSB, including a null value that clears an earlier selection.
The runtime retains the selected decoded root until frame consumption, so
the name stays valid without copying it each frame.

When a sky bank exists, `79A870` applies the selected MOSB through `7F31C0`:
slot zero receives that model and the stored DayNight `+0x9C` weight, slot one
is cleared, and slot two retains its current DBC result. `7F16F0` computes
the weight by multiplying the WMO boundary distance by float `0.04`, clamping
to zero through one, and storing f32. Fog color blending retains its separate
extended-precision calculation. MOSB aliases share the ordinary sky model
cache and preserve the first request's animation phase flags. Authored names
are resolved once, including empty/invalid requests.

`7F09B0` intersects the sky bank with the viewport through `48ED60` and requires
positive area. A missing bank or any submerged camera liquid suppresses sky
drawing. Palette model requests still occur; resident model animation, phase
updates and random consumption stop until the sky is visible again. WMO model
requests require the bank, but precede the camera-liquid and viewport gates.

`WorldSkyWindow` carries that rectangle to all sky queues. The Vulkan scissor
uses the original D3D9 backbuffer calculation from `6A38D0`: lower edges add
0.5 and upper edges add 1 before truncation and attachment clipping. The sky
projection stays full-sized, and the world scissor is restored before WDL and
ordinary geometry. A closed sky bank clears to camera indoor fog; an open bank
with camera liquid clears to ordinary fog; the ordinary visible sky uses black.
An admitted but fully hidden sky scene can present its background without any
draw packets.

`world_sky_visibility_oracle.py` captures 160 draw-gate/intersection/scissor
cases, five native slot replacements and 28 stored boundary fades. Decoded
WMO tests cover empty MOSB, independent MOGI/MOGP flags, primary/secondary
selection, portal windows and per-frame reset. The compositor GPU test covers
24 frames, including portal edges, background color, hidden sky, changing bone
prefixes, and restoration of world drawing beyond the sky scissor. A synthetic
runtime sky test exercises MOSB aliases, inherited phase flags, opacity pixels,
hidden-scene random/phase retention and return to the three DBC slots.

The global override at `D38B5C` is a separate owner. `7ECEC0` selects its
Light condition, and `7F3230` resolves that condition against the global Light
row. Its provider `4F7020` also drives screen effects and manual fog. That
provider, manual-fog sky enable/clear behavior, precipitation, and combined
live-world appearance checks remain separate completion work.

Build 79 packages revision `d45c0a68`. Workspace tests, formatting and Clippy
passed; the installed executable matches the packaged SHA-256
`9E64ECAB22BB99B2287329E927674FAED8ADD7BBDF4D6252AAD7EABA5DE9C20B`.
An optimized offline world smoke run completed 1,260 frames across seven
stationary, travel and settling phases without logged renderer errors. Captures
confirm world rendering through those transitions and retain the existing large
purple distant-terrain silhouettes as an unresolved appearance issue. This run
does not establish populated-world or interior parity, and capture timings are
not performance evidence.
