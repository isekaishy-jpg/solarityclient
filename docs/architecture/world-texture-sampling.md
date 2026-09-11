# World file texture sampling

The graphics owner reads saved `textureFilteringMode` and `BaseMip` before
preparing file samplers. Terrain diffuse textures and ordinary M2 file textures
now use that policy. The existing WMO, liquid and ground-detail consumers receive
the same typed values when their world frame is created. Previously terrain
diffuse sampling disabled anisotropy, M2 sampling constrained every image to mip
zero, and world-frame construction always requested 4x filtering.

M2 mesh, particle and ribbon preparation uses the shared file-sampler entry point,
including entity bodies, equipment and scenery. Authored U/V wrapping is retained.
Explicitly unmipped providers have a separate entry point and immutable sampler
identity. Terrain's material atlas keeps its own clamped, unmipped sampler.

## Native contract

`4048F0` maps the six filtering values through `AB6128` and `AB6140`.
`4B61C0` chooses bilinear, trilinear or anisotropic filtering, falling back to
supported capabilities. `4B6230` caps the requested anisotropy to the device limit.
The shared file-loader prefix at `4B97A5..4B97E7` replaces the requested filter
unless loader option bit zero explicitly preserves it. It then packs the
effective anisotropy while retaining the independent addressing bits.

This later replacement matters: the initial M2 request at `681BE0` alone does
not establish the sampler used by an ordinary file texture. The former mip-zero
implementation omitted this shared loader stage.

Vulkan samplers cap anisotropy to the enabled device feature and limit, use
linear minification/magnification, and select point mips only for bilinear mode.
`BaseMip` chooses the first authored mip, zero or one. Prepared descriptors retain
immutable identities; a renderer rejects a different global policy after file
samplers exist. Reapplying the identical policy is permitted.

## Evidence and boundary

`world_texture_filter_oracle.py` captures 960 original-code combinations of mode,
capabilities, device limit, explicit-loader option and requested flags. The unit
comparison checks 384 applicable ordinary-file and explicit M2 cases; the other
captured cases cover unsupported trilinear capability or independent explicit
texture classes.

The GPU integration test uses authored green mip zero and red lower mips. Both
terrain and M2 draws select the expected image with `BaseMip` zero/one. Alternating
explicit, file and explicit M2 draws verifies that sharing an uploaded texture
does not mutate a retained descriptor. Sampler reuse, capability limits and
late configuration rejection are also checked.

The workspace tests, strict Clippy checks and optimized capture build pass.
A controlled 2560 by 1440 Durotar replay loads saved 16x filtering and mip zero,
with the same camera and equipped NPCs used for the UI-scale capture. Stationary
and orbit views retain equipment, UI layout and world rendering. The comparison
still shows the separate terrain texture-density and bright-lighting differences;
the sampling change does not close those findings.

Saved settings are applied at graphics initialization. Live graphics settings,
the native BaseMip reload callback, and restart/apply controls remain part of
the core UI/settings owner work. Sampling validation does not establish complete
world lighting or all terrain material permutations.
