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

Before either path begins, the asset boundary computes the exact combined
staging footprint. The renderer allocates one vector, appends each mip in
authored order, and verifies the final byte count before creating Vulkan
resources. Stock-compatible undersized DXT tail mips are zero-padded to their
block-rounded copy footprint, matching the established decoder behavior. This
avoids capacity-growth copies and prevents HD DXT content from expanding by
roughly four to eight times in staging and device memory.

Staging bytes are temporary. The renderer retains only the device-local
image and its view after the synchronous transfer retires; the shared asset
cache continues to own the parsed source for other consumers.
