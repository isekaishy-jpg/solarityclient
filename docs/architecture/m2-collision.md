# Build-12340 M2 collision

Dedicated M2 collision is independent of animated render geometry. Header
offsets `0xD8`, `0xE0`, and `0xE8` contain a triangle-list index array,
model-space positions, and one model-space face normal per triangle.

The asset boundary decodes these arrays directly and requires all three to be
present together. Indices must form complete triangles and reference existing
positions; the face-normal count must equal the triangle count. Authored finite
normals are preserved without normalization or geometric reconstruction.

The dependency parser sees these pairs hidden in the temporary in-place header
view, avoiding redundant raw byte buffers for larger same-path HD models. The
decoded collision mesh remains shared by asset identity, while each MDDF or
MODD placement owns only its transform and broad-phase bounds.

The current camera trace is intentionally two-sided and does not consult the
authored normal for culling. A later behavior change requires build-12340
executable evidence; retaining the data now prevents that work from needing a
format-layer fallback.
