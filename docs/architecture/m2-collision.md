# Build-12340 M2 collision

Dedicated M2 collision is independent of animated render geometry. Header
offsets `0xD8`, `0xE0`, and `0xE8` contain a triangle-list index array,
model-space positions, and one model-space face normal per triangle.

The asset boundary decodes these arrays directly and requires all three to be
present together. Indices must form complete triangles and reference existing
positions; the face-normal count must equal the triangle count. Authored finite
normals are preserved without normalization or geometric reconstruction.

The owned body decoder reads these pairs once and does not retain a redundant
raw collision buffer for larger same-path HD models. The decoded collision mesh
remains shared by asset identity, while each MDDF or MODD placement owns only
its transform and broad-phase bounds.

An empty render box does not imply an empty collision mesh. `0x007BDB10`
detects render bounds inverted on all three axes and registers a zero-size
render box at the placement position. It still transforms the dedicated
collision box and retains its triangles. The stock Orgrimmar
`KL_AUCTIONHOUSECOLLIDE.M2` uses `+FLT_MAX` minima and `-FLT_MAX` maxima for
this render sentinel. Rejecting it as an invalid placement caused world entry
to exit even from the Drag, elsewhere in the same loaded ADT.

`PlacedM2Collision` applies that rule during admission and transform updates.
The portable regression verifies the point registration, retained movement
faces, and camera obstruction after moving the placement. Installed-data
validation reproduces the affected tile through ordinary runtime residency:

```text
cargo run -p solarity-runtime --example validate_world_scene -- <Data> enUS 1 1895.34 -4508.84 26.488
```

This command admits 2,040 M2 placements from 302 sources at the reproduced
position. It checks CPU terrain/model residency; it does not verify the network
handoff, GPU presentation, or interactive movement.

The current camera trace is intentionally two-sided and does not consult the
authored normal for culling. A later behavior change requires build-12340
executable evidence; retaining the data now prevents that work from needing a
format-layer fallback.

Movement now uses `PlacedM2Collision::append_movement`, following native
`0x0082EC30`. It transforms dedicated vertices, applies the world-box outcode
test in authored face order, and transforms authored normals using independently
normalized placement axes. It does not renormalize the authored normal itself.
See [movement world geometry](movement-world-geometry.md) for executable evidence,
the independent matrix boundary, and remaining runtime ownership.
