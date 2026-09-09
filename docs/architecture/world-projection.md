# World projection precision

Build 12340 projects camera-space vertices through a separate projection
matrix. Combining world-view and projection before transforming world-space
vertices changes floating-point cancellation at large map coordinates. M2
vertices additionally lose local surface separation when their placement
translation is added in the shader before the camera translation is removed.

## Stock evidence

The original archive shaders establish the transform order directly:

| Shader | Transform instructions | SHA-256 |
| --- | --- | --- |
| `Shaders/Vertex/vs_2_0/Terrain.bls` | View in `c0..c3`, then projection in `c4..c7` | `3bedcdfa3f183363739338cbac8abd698210cc253b10a3330f1c5778e517186c` |
| `Shaders/Vertex/vs_2_0/MapObjDiffuse_T1.bls` | Model/view dot products with `c31..c33`, then projection dot products with `c2..c5` | `67374f79748afca9797fa5e5a29fcd22f97cd71bc662904628d384fd4b90fd6d` |
| `Shaders/Vertex/vs_3_0/Diffuse_T1.bls` | Model/view dot products with `c31..c33`, then projection dot products with `c2..c5` | `e552498ae61b769c49ba87fd9ea8ff8c9d682df1b09ddc3802f6354e7be42f08` |

The unchanged first Terrain shader executes through D3D9 `ProcessVertices`
in `tools/ghidra/world_projection_oracle.py`. Its portable fixture records
72 cases: three world locations, eight yaw angles, and distances of 8, 30,
and 100 world units. Each row retains the exact input float words and native
post-viewport depth and reciprocal W. The harness rejects a shader whose hash
differs from the pinned input.

## Renderer contract

Terrain, liquid surface triangles, particles, and ribbons retain distinct
view and projection matrices. M2 materials reuse their CPU-composed model/view
matrix for position projection as well as environment coordinates. WMO
materials append a CPU-composed model/view matrix at frame upload. GLSL
`precise` intermediates preserve the boundary between those stages. Existing
world positions remain available for lighting and shadow calculations.

Every caller supplies projection and view separately, including Glue,
portraits, per-instance lighting scenes, and sky models. Sky depth compression
belongs to projection; camera translation removal remains in its view/model
transform. This change does not change the camera projection parameters,
depth format, or depth comparison.

## Regression coverage

`tests/stock_seed/projection.rs` compares the serialized terrain and M2 scene
matrices against all native captures. It allows two float epsilons in depth
for shader/host arithmetic differences and verifies that the previous combined
calculation diverges substantially in more than ten fixture cases.

`tests/stock_seed/model/projection.rs` renders two opaque surfaces separated
by 0.00025 local units at the origin, in Durotar, and at map-edge coordinates.
Eight rotations per location exercise actual M2 shaders and Vulkan depth
testing. The farther red surface is submitted after the nearer green surface;
the captured center pixels must stay green.
Reinstating the previous world-position/combined-projection calculation makes
this test fail at `(16000, 16000, 16000)`: the farther red surface overwrites
the nearer green surface. The separate model/view calculation passes all
24 placements and rotations.

These focused checks establish transform precision. They do not establish
that every reported camera-dependent visual defect shares this cause. Live
retests remain necessary for the reported roof, prop, shoreline, tower-base,
and head/helmet overlaps; distant-landscape behavior and zeppelin lighting
also require their own evidence.
