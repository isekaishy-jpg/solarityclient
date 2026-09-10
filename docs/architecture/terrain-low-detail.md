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
distance `camera.farclip - 50` and far distance `ADEECC * CD7748`. Registration
at `78E61D` installs `horizonFarclipScale` with default `4.0`; callback `78D7C0`
clamps it to `3..6` and setter `77F4A0` writes ADEECC. Its image initializer of
one is therefore not the runtime default. The previous implementation missed
that setter and clipped the horizon to the main camera's far plane. At the
default effective distance of 777, the corrected horizon covers `727..3108`
instead of `727..777`. Both normal play and the offline benchmark read the
live CVar before frame preparation. `795F80` selects viewport
depth `0.998046875..0.9990234375`. The fixed-function terrain pass sets fog
start/end to zero/one and uses the packed DayNight color at offset `8C`.
The Vulkan replacement keeps view and projection separate and outputs this
fully fogged color. `795F80` changes projection while retaining the main GX
view. The replacement therefore carries the source camera's retained direction
instead of subtracting its rounded one-unit world target. A regression checks
identical main/horizon views across yaw and large world coordinates.

The runtime supplies the ordinary fog bank published at DayNight `+8C`.
`7831A0` calls `7816F0`, which calls `7F3920` (including `7F3230` depth
darkening), `7F1010`, `7EEA80`, and finally `7F16F0`. That final publication
overwrites the temporary darkened `+8C` with the undarkened palette or retained
manual fog color before world drawing. Native liquid bank overrides also
apply here. The camera's blended interior color remains separate at `+A0`.
The previous runtime incorrectly retained the earlier darkened palette word,
making distant terrain disagree with underwater and Nether fog.
Ordinary world queues use their existing documented
`0..0.94` interval; Glue frames retain their own depth interval. Sky keeps its
existing reserved projection and is drawn before the horizon.

The terrain worker retains one immutable CPU bank across tile jobs, including
a cached absent-WDL result. The world renderer uploads each map once. Every
submitted frame slot retains its map identity until its fence retires; the
shared GPU registry releases a map only when no CPU owner or submitted slot
retains it. This avoids copying map geometry into each frame slot or uploading
it again when the resident ADT changes.

## Evidence and checks

`tools/ghidra/terrain_horizon_fog_oracle.py` executes native `7F3230` depth
arithmetic, the `7816F0` publication sequence through `7F16F0`, and `7D5E70`
through its GX fog-state stores. Its 108 cases retain the intermediate color,
ordinary color and actual published GX color for six palettes, three depths,
dry/wet cameras and normal/two manual fog colors. Sky/model/device providers
are controlled; the already captured depth result stands in for the full sky
producer. WMO queries report no interior. The runtime environment regression
checks the captured GX colors for dry, submerged, manual and indoor frames,
with existing separate native MFOG fixtures covering the interior banks.
The fixture SHA-256 is
`716e9cc338312e344a90be3c9e188b34b039f271ef15e13b6bdb89874c84e31d`.

Three optimized real-archive replays completed 1,260 frames each. Normal and
Nether runs used map 1 at `(1300, -4530, 50)`, camera distance 25 and a
200-unit outward/return travel offset. The Nether run selected that owner in
all 1,260 frames. Its matching settled capture no longer contains the blue
distant silhouettes against pale manual fog; nearby terrain and colored UI
remain visible. The normal capture still contains conspicuous distant shapes,
so the color correction does not establish complete horizon appearance.
The water replay at `(1100, -5500, -20)` moved vertically by 60 units and back,
recording 1,015 wave/liquid-type-2 frames and 245 normal/dry frames. Inspected
submerged captures retain textured seabed, the underwater effect and UI.
All three logs contain no renderer warnings or errors. These captured runs
validate presentation and transitions; they are not performance measurements.
The full workspace passed 1,205 tests with 23 ignored. After correcting the
fixture reader's error handling, the focused environment regression passed
again and workspace Clippy passed for all targets/features with warnings denied.
Build 85 was installed from source revision
`bb66cc63d12abc8a82df686377ad612cc0b46f0d`. Installed and packaged executables
both report `0.0.3a`, build 85, and share SHA-256
`a931c923c22d4315f2039ab1e51e51fe591103df7662b3d7ad78721d75369ec0`.
The reported dirty flag reflects the build-number reservation before compilation.

`tools/ghidra/terrain_low_detail_oracle.py` executes `7CC310`, `7D5150` and
`7D5240` from the fingerprinted build-12340 executable. Archive reads,
allocation and GX buffer lock/unlock are controlled boundaries. Its portable
fixture covers four tile locations and all-clear, all-marked, checkerboard
and diagonal masks. Tests compare all bounds, 545 positions, native vertex
colors, face-bank counts and 3072 indices exactly.

`tools/ghidra/terrain_horizon_projection_oracle.py` captures the actual CVar
registration arguments and executes the original callback, setter and horizon
projection for 80 cases. The only substituted inputs are the decimal parser's
float result and the camera's virtual FOV getter. Tests compare the effective
scale and near/far float stores exactly, and the Vulkan-converted projection
coefficients within the existing common camera builder's float precision.

`tools/ghidra/camera_fog_handoff_oracle.py` separately executes `4F8410`
through its camera-to-DayNight far-plane store, then the original exterior
`7ECD80`/`7F16F0` fog arithmetic. With the installed midnight Durotar sample
at map 1, `(1300, -4530, 50)` (raw fog end `888.8889`, ratio `0.5`), changing
the registered horizon scale between 3, 4 and 6 leaves the camera/fog far
distance at 777 and the final fog at start 388.5, end 777, exponent 1.
The ordinary fog distance must therefore remain independent of the horizon
multiplier. This checks the exterior handoff and arithmetic; it does not
establish that every reported fade or camera-dependent visual is corrected.

Paired real-archive Vulkan captures at `(1300, -4400, 40)` and
`(1300, -4530, 50)` show distant mountain ranges that were clipped by the
previous projection, with the foreground preserved. These offline captures
omit server GameObjects and do not measure gameplay performance.

The hidden Vulkan test uses independent ray/plane coverage to check 32
horizon frames across map changes and repeated frame-slot reuse. It exercises
both sides of marked faces, yaw changes, translated coordinates, near/far
clipping, sky preservation, more than 1000 sampled pixels beyond the ordinary
far plane, and ordinary geometry at almost-far depth covering the horizon.
The real archive validator reads 988 Kalimdor, 687 Eastern
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
