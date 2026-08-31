# Build-12340 external M2 skin profiles

Version-264 M2 render geometry uses mandatory external `Model00.skin` through
`ModelNN.skin` companion files. Each companion is resolved independently by
the ordinary MPQ file stack. A later patch archive can therefore replace an
M2, any of its SKIN companions, or both at the same stock paths.

The asset layer does not define an HD pack, HD model, or alternate HD format.
Down-ported HD packs contain the same logical files and layouts with larger
payloads. The 64-bit process accepts those files through the same cache and
decoder, retaining only the archive entry selected by normal precedence.

The exact WotLK SKIN layout has a 48-byte header containing five count/offset
pairs and the declared maximum bone count. The arrays are:

- 16-bit model-vertex lookup entries;
- 16-bit triangle entries indexing that vertex lookup;
- four byte-sized bone-palette indices per profile vertex;
- 48-byte submesh records, including the center-bone word at byte 18; and
- 24-byte material batch records.

The submesh `level` word supplies the high bits of `triangle_start`, so large
replacement geometry is not truncated at 65,535 triangle-list entries. Every
count and byte range uses checked native-width arithmetic before allocation.
Cross-array references and finite bounds are validated once at the asset
boundary. There is no format detection, repair path, or stock-to-HD fallback.

Decoding is owned by the asset crate rather than a temporary dependency model.
This avoids duplicate vectors and conversion passes for the largest geometry
payloads while preserving the exact fields consumed by rendering.
