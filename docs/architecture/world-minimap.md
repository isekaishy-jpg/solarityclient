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

The live outdoor minimap now joins that projection through a native
`UiRenderSource::Minimap` composition slot. The stock constructor (`0x0057DCA0`)
registers ARTWORK and creates the player texture before XML children. The texture
constructor's final argument is a visibility flag (`0x00487ED0`), not a draw
sublevel. The slot therefore places terrain and the player arrow before authored
textures in the same ARTWORK band, while retaining the owning frame's order,
visibility, alpha, inherited scale, and ScrollFrame clipping.

Per-widget arrow paths and dimensions enter the retained Lua snapshot, including
XML inheritance and `CreateFrame` templates. Lua setters dirty that widget when
its state changes. `SetMaskTexture` selects the native scene's shared mask and
increments its revision. The runtime reads these typed settings and the resident
player's world transform; it does not run Lua or rebuild FrameXML for player
motion. `rotateMinimap` selects a map heading following player facing and keeps
the arrow upright.

`RuntimeMinimapScene` requests only the current neighborhood, mask, and arrow.
MPQ reads and BLP decoding run on the bounded CPU executor, with an independently
mounted archive stack retained between jobs. Queue saturation defers admission.
The renderer uploads completed sources once and retains their handles. Absent
images are remembered, and unresolved or unmapped locations never reuse stale
tiles. Initial requests participate in loading-screen readiness. Movement and
material changes reuse each minimap's GPU mesh; steady state does no geometry
work. The compositor retains the complete UI draw list and patches only minimap
draw ranges when their counts and surrounding UI remain unchanged.

An archive-backed runtime GPU test covers deferred admission, asynchronous
publication, native layer ordering, circular masking, player placement, movement
and map rotation, hide/show mesh reuse, unmapped locations, and missing images.
UI tests cover authored dimensions and paths, inherited scale, setter updates,
initially hidden widgets, template creation, and the scene's shared mask.

An actual Orgrimmar FrameXML capture confirms the outdoor map and player arrow
inside the stock circular mask. Unrelated UI revisions retain the minimap mesh
without uploading it again. An uncaptured 1,800-frame-per-phase 2560-by-1440
offline run measured 7.44 ms stationary, 7.00 ms orbiting, and 7.81 ms with pointer
activity. These measurements do not establish live-server performance or the
1,200 FPS target.

File-only XML textures such as `MinimapBorder` receive the parent-filling anchors
created by stock `0x00815F40` / `0x004830E0`. Initial region resolution and dynamic
XML templates both retain these as ordinary TOPLEFT/BOTTOMRIGHT constraints,
including when a texture declares Size without anchors. Inherited explicit
anchors are preserved. Lua `ClearAllPoints` removes the defaults; `CreateTexture`
without XML starts unanchored. The regression covers these cases and resulting
render geometry.

Indoor group selection, compass-ring rotation, native tinting, tracking, and
network pings still need integration. Runtime geometry must drive the retained
indoor/outdoor selection; the current production composition selects terrain tiles.
Texture handles remain
resident for the world UI lifetime, matching the existing UI residency policy.
