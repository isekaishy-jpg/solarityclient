# Build-12340 M2 model body

The asset crate reads the exact version-264 `MD20` header directly. The base
header is `0x130` bytes; global flag `0x8` requires the `0x138`-byte form whose
final count/offset pair names the texture-combiner table. Later chunked M2
versions and later record shapes are rejected at this boundary.

The retained unanimated body owns only data consumed by the client:

- the model name, global flags, render bounds, and collision bounds;
- exact 48-byte vertices, including both texture-coordinate sets;
- 16-byte texture declarations and their nested archive paths;
- four-byte render-flag/material records;
- the optional texture-combiner table; and
- the separately decoded collision and fixed lookup arrays.

Counts and offsets are validated with checked native-width arithmetic before
allocation. Vertices are decoded directly into the final `M2Vertex` vector;
raw bone weights and indices are preserved without repair. Texture kinds and
blend modes outside build 12340's known values fail instead of receiving a
later-version substitute.

The production asset path no longer constructs a dependency-owned M2 object or
its auxiliary raw buffers. This matters for down-ported HD packs: their files
win ordinary MPQ precedence at the same paths, and their larger geometry is
allocated once in the 64-bit process. `wow-m2` remains a development-only
fixture serializer and is not linked into the normal asset dependency graph.
