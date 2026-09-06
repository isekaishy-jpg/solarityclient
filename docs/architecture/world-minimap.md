# World minimap

The asset boundary loads `Textures/Minimap/md5translate.trs` through normal MPQ
precedence. Directory records and unpaired lines are skipped. Logical names use
case-insensitive archive path identity, and a later duplicate replaces the earlier
mapping. A missing table yields an empty catalog; archive read and invalid path
failures remain errors. This follows build 12340's loader at `0x007F6540` and its
shared name lookup at `0x0055F4D0`.

Outdoor terrain lookup (`0x007F5240`) combines the Map.dbc directory and ADT tile
indices using `%s\map%d_%02d.blp`, then resolves the translation to a file beneath
`Textures/Minimap`. Missing entries remain unresolved. Indoor WMO entries share
the same catalog; `0x007F5070` uses `%s_%03d_%02d_%02d.blp` for the WMO/group/tile
name before that lookup. Indoor group selection and composition are separate
runtime responsibilities.

Archive-backed tests verify replacement by a patch table, duplicate logical
names, case normalization, both terrain and WMO entries, missing tables, missing
entries, and rejection of invalid archive paths. A direct check against the local
client archives resolved and parsed four Kalimdor tiles around the Drag and one
Orgrimmar lava-dungeon group tile, all 256 by 256 BLP images.

The stock minimap scene has separate indoor and outdoor zoom indices. The zoom
setter `0x007F3AE0` clamps an unsigned index above four to five. Its distance query
`0x007F3B90` supplies outdoor radii from `[14, 12, 10, 8, 6, 4] * 0.5 * 33.333332`
and indoor radii `[150, 120, 90, 60, 40, 25]`. The ordinary outdoor path admits four
tiles around the player (`0x007F5760`) and composites their texture and mask
coordinates (`0x0057D5F0`, `0x0057E540`). The default mask is
`Textures/MinimapMask`; stock FrameXML also names an M2 player arrow.

The renderer also accepts an independent archive alpha mask on ordinary UI
texture quads. `UiRenderMask` retains the image identity and its logical rectangle;
each tile keeps its own source UVs while the vertex shader derives mask UVs from
that rectangle. Two compatible sampled-image descriptor sets share the ordinary
UI pass. This requires neither a composed CPU image nor another offscreen pass.
Retained draw translation carries both geometry and mask; mask changes split
material batches and invalidate incompatible retained resources. UI asset plans
deduplicate source and mask requests, including promotion to blocking residency.
GPU capture tests verify adjacent tiles sharing an asymmetric mask, transparent
and half-alpha regions, unchanged source RGB, movement, and a subsequent ordinary
UI draw. Validation rejects missing/mismatched masks and degenerate rectangles.

The visible minimap is still incomplete. The retained Minimap widget snapshot,
runtime tile residency and projection, indoor group selection, player arrow,
rotation, tracking, and pings still need integration. The existing Lua zoom setter
also needs to be joined to the recovered shared zoom state.
