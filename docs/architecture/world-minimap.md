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
`Textures/MinimapMask`. Although stock FrameXML names legacy player/arrow M2
attributes, those attribute names are absent from the build-12340 executable.
The native XML loader (`0x0057BEA0`) reads `minimapPlayerTexture`, defaults to
`Interface/Minimap/MinimapArrow.tga`, and assigns an ordinary texture region through
`0x004859E0`. Marker integration must follow that texture path. The width/height
setters (`0x0057E280`, `0x0057E1C0`) resize that region; the executable does not
register corresponding width/height getters.

`UiMinimapState` now owns both zoom indices and the indoor/outdoor selection,
shared by every Minimap widget and exposed to the world renderer. Both saved
CVars (`minimapZoom`, `minimapInsideZoom`) default to `3`: registration at
`0x0051D9B0` uses integer type 4, saved flag `0x20`, no bounds, and no callback.
Scene startup (`0x007F6730`) copies their values. Direct `SetCVar` calls therefore
do not alter the active zoom. Changed `Minimap:SetZoom` calls write the selected
CVar through the existing profile persistence path, matching `0x00766940`;
setting the existing zoom does not rewrite it. The Lua wrapper (`0x0057BFD0`)
truncates to an x87 signed 64-bit integer, passes its low unsigned 32-bit word,
and clamps to five. Invalid conversion inputs produce a low word of zero.

`FrameManager::set_minimap_indoors` selects the retained mode before dispatching
`MINIMAP_UPDATE_ZOOM`, once per mode transition. The renderer can sample a
revision and the native world radius without querying Lua. Integration tests
cover shared widgets, fractional/negative/overflow inputs, saved settings before
OnLoad, independent zoom modes, change-only persistence, and event ordering.
The fabricated player-width/height Lua getters have been removed.

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

`MinimapView` supplies the outdoor neighborhood and world-to-UI projection.
World +X is north/up and +Y is west/left; a positive map heading rotates terrain
clockwise beneath the fixed mask. The four tiles follow the native half-tile
neighborhood and corner order, including map-edge handling. Both sides of a tile
seam derive their common world coordinate from the same integer grid edge.
Archive-resolved terrain quads retain full image UVs, independent mask coordinates,
and the viewport scissor. The same projection accepts world-space corners for
future runtime-resolved WMO tiles.

UI quads can now retain transformed corners as well as ordinary rectangles.
Initial mesh preparation and both retained replacement paths preserve those
positions with the existing vertex and index ABI. Bounds/scissor culling considers
all four corners; validation rejects non-finite replacements before mutation.
The player marker uses a fixed rectangle and native `0x00483120` sampling:
unit-radius UV corners around `(0.5, 0.5)`, offset by facing minus map heading
minus pi/4. Its dimensions do not expand with rotation. Stock `Minimap.lua`
sets both dimensions to 40; the archive arrow itself is 32 by 32.

Projection tests cover cardinal directions, center/scale changes, exact shared
edges, marker UVs, and retained corner updates. GPU captures verify four differently
colored tiles at zero, pi/4, and pi/2 headings, with a fixed circular mask, no
viewport spill, and no interior seams. An additional offline render of the local
Orgrimmar archives at `(1562, -4405)` resolves the four terrain images, stock mask,
and arrow, and visually confirms their composition in both minimap modes.

The live minimap is still incomplete. The retained Minimap widget snapshot,
runtime tile residency, draw ordering, and player/map updates still need to be
joined to this projection. Indoor group selection, native tinting, tracking,
and pings also need integration. Runtime geometry must drive the retained
indoor/outdoor selection.
