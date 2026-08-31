# BLP texture residency

BLP files are selected through the ordinary MPQ stack. Higher-resolution packs
replace the same virtual paths and therefore retain the same cache and Vulkan
resource identity as stock textures.

The asset cache keeps parsed authored mip payloads in their compressed form.
It does not eagerly expand every mip of an HD replacement and does not retain a
parallel low-resolution source. Consumers decode only the mip data required by
their stock operation.

GPU admission uploads every authored mip. Before decoding, the asset boundary
computes the exact total RGBA8 byte count with checked native-width arithmetic.
The renderer allocates the combined staging vector once at that size, appends
each decoded mip in authored order, and verifies the final byte count before
creating Vulkan resources. This avoids capacity-growth reallocations and copies
whose cost scales with larger HD mip chains.

Decoded staging bytes are temporary. The renderer retains only the device-local
image and its view after the synchronous transfer retires; the shared asset
cache continues to own the compressed source for other consumers.
