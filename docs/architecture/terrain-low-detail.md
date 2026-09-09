# Low-detail terrain

The world renderer loads the map-wide WDL independently of resident ADTs.
Native `7CC310` opens `World/Maps/<map>/<map>.wdl`, addresses its 64-by-64
MAOF table, and retains 545 signed heights for each MARE tile. The optional
MAHO words divide faces into two culling banks; they do not remove cells.
MWMO, MWID and MODF also describe distant world-model placements.

`7D5150` uploads the 17-by-17 corner grid followed by 16-by-16 cell centers.
`7D5240` emits four triangles per cell, first for clear mask bits and then
for marked bits. Clear cells disable culling; marked cells cull clockwise
faces. `7D5E70` skips a tile entirely when its clear bank is empty. The CPU
mesh preserves the original extended-precision stepping, stored base corners,
and separately rounded lower bounds.

`791170` uses the ordinary camera orientation and vertical FOV, with near
distance `camera.farclip - 50` and far distance `ADEECC * CD7748`. ADEECC is
initialized to one and has no executable writers. `795F80` selects viewport
depth `0.998046875..0.9990234375`. The fixed-function terrain pass sets fog
start/end to zero/one and uses the packed DayNight color at offset `8C`.
The Vulkan replacement keeps view and projection separate and outputs this
fully fogged color. Ordinary world queues use their existing documented
`0..0.94` interval; Glue frames retain their own depth interval. Sky keeps its
existing reserved projection and is drawn before the horizon.

The terrain worker retains one immutable CPU bank across tile jobs, including
a cached absent-WDL result. The world renderer uploads each map once. Every
submitted frame slot retains its map identity until its fence retires; the
shared GPU registry releases a map only when no CPU owner or submitted slot
retains it. This avoids copying map geometry into each frame slot or uploading
it again when the resident ADT changes.

## Evidence and checks

`tools/ghidra/terrain_low_detail_oracle.py` executes `7CC310`, `7D5150` and
`7D5240` from the fingerprinted build-12340 executable. Archive reads,
allocation and GX buffer lock/unlock are controlled boundaries. Its portable
fixture covers four tile locations and all-clear, all-marked, checkerboard
and diagonal masks. Tests compare all bounds, 545 positions, native vertex
colors, face-bank counts and 3072 indices exactly.

The hidden Vulkan test uses independent ray/plane coverage to check 32
horizon frames across map changes and repeated frame-slot reuse. It exercises
both sides of marked faces, yaw changes, translated coordinates, near/far
clipping, sky preservation and ordinary geometry at almost-far depth covering
the horizon. The real archive validator reads 988 Kalimdor, 687 Eastern
Kingdoms, 800 Outland and 1131 Northrend WDL tiles.

## Remaining horizon work

This change submits WDL terrain. The asset API retains MODF placements, but
the runtime does not yet submit their far WMO groups. Northrend has four such
placements in the installed archive set. Native `795F80` admits only loaded
exterior groups and `7ABAC0` applies its own aggregate/batch and texture-ready
rules; that residency and draw path still needs integration.

Native `7CC0B0` additionally uses the scene occluder spheres (`7CCE00`) and,
under the world occlusion flag and camera-angle gate, the terrain horizon
buffer (`78FDC0`). The new pass performs frustum selection and normal depth
occlusion; those CPU occlusion rejection paths remain part of the broader
world visibility/performance work. The GPU checks establish the implemented
draw contract, not completion of every reported landscape defect.
