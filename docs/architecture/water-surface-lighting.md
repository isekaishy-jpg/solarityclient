# Water surface lighting

The exterior light follows `0x007EEA90`'s executable-owned day tables and
periodic approximation. `0x00834AE0` normalizes the authored light ray;
`0x00834F60` adds its ambient, diffuse and specular colors to the active
lighting query and retains the ray direction. `0x008A38B0` transforms that
direction into view space and writes liquid vertex constants c33 through c36.

The common runtime environment exposes the opposite, surface-to-light vector
for terrain, WMO and M2 shaders. The liquid boundary must negate it before
the view transform. The original water shader negates its incoming ray in
the normal dot product and also uses that ray to construct the specular
half vector. Passing the common vector directly suppresses upward-facing
water's direct illumination and points its specular response the wrong way.
The private WMO interior light is a separate native ray and does not pass
through this conversion.

`tools/ghidra/water_light_uniform_oracle.py` executes the original day-table,
normalization, single-light accumulation and liquid constant-writing routines.
Its 192 captures span a complete day, the interpolation boundaries and six
cardinal camera orientations. The controlled inputs are a resident camera
matrix, a single exterior light and the enabled-fog provider. The runtime
test loads authored WDBC colors, constructs the realm clock and final camera,
then compares the actual liquid descriptor's direction, color and fog words.
The pre-fix test fails on the first directional component with opposite signs.

This establishes the exterior directional-light boundary. Animated scene lights
now have separate registration, query and upload coverage in
[world scene lights](world-scene-lights.md). Fog policy and sky composition are
covered separately in [world fog](world-fog.md) and [world sky](world-sky.md).

## Shared day bands and local overlays

`world_light_sampling_oracle.py` captures 575 original records: 272 cyclic
band samples, four complete parameter inputs, 68 parameter palettes and 231
ordered local overlays. Only the decoded WDBC band provider is substituted;
`7EB070`, `7EAEF0`, `7EBFF0`, `7ECD80` and `7ED4C0` execute original code.
The fixture covers midnight wrapping, single and sixteen-key bands, exact
keys, half-channel rounding, repeated skyboxes and overlapping local volumes.

Color interpolation stores to float before nearest-even integer conversion.
Fog distance scales both source keys by the native float `1/36` before
interpolation. Local volumes then overlay the current packed palette in
order; reducing earlier weights and summing normalized colors loses each
intermediate byte quantization. `7ED2D0` also subtracts exactly one half before
nearest-even conversion, including at full weight. The asset sampler retains
these boundaries and unpacks colors only after composition.

All eighteen color channels, the cloud-type ID and its independent weight
are preserved. Local cloud density blends, while the other three sky scalars
retain the global palette's values. Skybox slots accumulate repeated IDs up
to one without attenuating earlier slots. This establishes sampling under
the native default fog mode; camera-distance fog remapping and spatial
local-light registration have separate evidence in the linked documents. Weather palette composition
now has separate native captures and runtime coverage in [world sky](world-sky.md).

The shared palette now also drives the [native sky dome](world-sky.md), including
the fog-colored lower hemisphere. The Vulkan background and camera-relative
geometry, procedural clouds, celestial strips, stars and authored skyboxes are
present in their native compositor queues.
