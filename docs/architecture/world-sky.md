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

This adds the base gradient renderer. Procedural clouds, celestial textures,
stars, authored skybox models, weather overrides and WMO-specific sky visibility
remain separate portions of the lighting/sky slice.
