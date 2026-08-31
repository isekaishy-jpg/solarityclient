# BLP texture residency

BLP files are selected through the ordinary MPQ stack. Higher-resolution packs
replace the same virtual paths and therefore retain the same cache and Vulkan
resource identity as stock textures.

The asset cache keeps parsed authored mip payloads in their compressed form.
It does not eagerly expand every mip of an HD replacement and does not retain a
parallel low-resolution source. Consumers decode only the mip data required by
their stock operation.

GPU admission uploads every authored mip. DXT1, DXT3, and DXT5 payloads remain
compressed and become BC1, BC2, and BC3 Vulkan images respectively. JPEG,
paletted, and raw BLP encodings are decoded to RGBA8 because Vulkan cannot
sample those file encodings directly. This is an encoding boundary, not an
adapter-dependent DXT fallback.

Before either path begins, the asset boundary computes each texture's exact
combined mip footprint. The renderer appends every requested texture and mip
in authored order to one exactly sized staging vector, padding only between
textures where Vulkan's destination texel-block alignment requires it. It
verifies the final byte count before creating Vulkan resources.

One admission batch uses one staging allocation, one command buffer, one queue
submission, and one fence retirement for every new path/color-space identity.
Requests retain their original order, while duplicate and already-resident
identities resolve to the existing image without another transfer. Registry
state changes only after the complete batch succeeds.

Stock-compatible undersized DXT tail mips are zero-padded to their
block-rounded copy footprint, matching the established decoder behavior. The
batch avoids per-texture submission overhead and capacity-growth copies, while
direct BC storage prevents HD DXT content from expanding by roughly four to
eight times in staging and device memory.

Staging bytes are temporary. The renderer retains only the device-local
image and its view after the synchronous transfer retires; the shared asset
cache continues to own the parsed source for other consumers.
