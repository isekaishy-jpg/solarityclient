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

This establishes the exterior directional-light boundary. It does not establish
local-light registration, fog-policy parity or the missing sky, cloud and
celestial rendering systems; those remain part of the water/lighting slice.
