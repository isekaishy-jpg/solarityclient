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

## Orgrimmar true-exterior admission correction

Build 126 still published a WDL frame whenever the resident map had low-detail
terrain. This bypassed an existing distinction in camera traversal: a WMO can
admit sky without opening any exterior terrain. The reported city view showed
flat teal mountain silhouettes behind the skyline, while the supplied stock
view showed sky.

Disassembly and Ghidra recovery against the pinned build-12340 executable
(SHA-256 `aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`)
establish the missing admission:

- `79A870` keeps independent sky (`ADF570..ADF580`) and true-exterior
  (`ADF58C..ADF59C`) banks. An unregistered camera opens the full exterior
  bank. A registered camera reaches `79A790` only when primary traversal
  opens the true-exterior bank.
- `79A790` calls `791980` after the ordinary exterior traversal.
  `791980` builds the horizon projection, passes the exterior window to
  `790E20`, and then calls `7CC0B0` to queue eligible horizon tiles.
- `790E20` interpolates the horizon frustum corners with that normalized
  window, including bounds outside the viewport. The later `795F80` draw
  consumes the queued list; its unconditional call does not authorize every
  map tile.

The runtime now retains the already-computed primary exterior window and
publishes the horizon frame through a focused scene module. A closed bank
publishes no WDL frame. An open bank supplies the horizon frustum's side
planes; sky visibility remains independent. This adds no scene scan, logging,
allocation, or GPU query. The outdoor case retains the existing tile loop.

Installed archive probes against Orgrimmar MODF 165042 at
`(1500, -4400, 35)`, `(1550, -4400, 35)`, and `(1600, -4400, 35)`, looking
along positive X, select camera groups 132, 132, and 133 respectively. All
three have visible sky and no true-exterior window. These repeatable WMO
probes omit ADT ray limiting and do not claim to reproduce the exact supplied
screenshot camera. The ignored installed-data regression exercises these
groups and verifies that they publish no horizon frame. Portable regressions
cover sky-only frame suppression, an open exterior bank, and horizon tiles
accepted/rejected by opposite portal windows.

Validation passed 1,358 workspace tests with 27 ignored, plus the ignored
installed Orgrimmar regression run explicitly. Workspace formatting and
all-target/all-feature Clippy with warnings denied passed. A 240-frame offline
Vulkan replay at player position `(1525, -4400, 35)`, camera distance 25,
pitch/yaw zero, realm hour 3 and 1280x720 shows clear sky behind the city
towers. The stationary final capture retains the foreground city geometry
without the teal silhouettes. This uses the offline fixture character, not
Soap's exact pose, and establishes no FPS change. The development example
needed a larger stack reserve in a disposable copied executable; the packaged
client is built normally.

Local evidence is retained under `target/orgrimmar-horizon-*`: stock owner and
clip decompilations, installed probes, workspace/installed test logs, and
`capture/stationary-0059.png`. Archive extracts and captures remain local.

Testing Build 127 packages revision
`2e3e3f257b984d94e9a5550ff713c687f922b272`. The installed and sealed
executables share SHA-256
`212c1f7511852caff37fbb28fdbeaff948e577d1c8ce4d7b0bb2152eaac6dafb`.
The dirty identity records the package build-number reservation. Installed
runtime DLL hashes and the Testing desktop shortcut were verified.

This correction does not establish when the reported appearance first changed.
The earlier horizon evidence above already records conspicuous distant shapes.
It also does not establish the uncertain outside-the-gate case: an exterior
camera still admits WDL and may require the native occlusion work below.

## Ordinary terrain admission follow-up

The user confirmed that Build 127 still displayed mountains inside the city.
Its earlier replay was not sufficient visual validation: the camera differed
from the supplied view, its far clip was 777 rather than the Testing profile's
1277, and short incremental streaming had not populated the surrounding ADTs.
The WDL correction above remains independently supported by stock evidence,
but did not close the ordinary-terrain submission path.

Further recovery from the same pinned executable shows that `79A790` installs
the ordinary exterior clip through `790AF0` before `799D40` selects ADT chunks.
`799D40` performs chunk rejection before reaching ground detail (`7D3FE0`) and
the eligible surface/water queues. The runtime previously used the full camera
frustum for ADT terrain, terrain liquids, and ground detail even when primary
WMO traversal closed exterior visibility.

Those submissions now share the retained primary exterior window with WDL,
using the ordinary camera projection for regular terrain. Preparation clears
old draw packets when the window closes; sky and admitted WMO geometry remain
independent. Terrain selection lives in a focused frame module, and the scene
module owns both exterior projections. This reuses the existing WMO traversal
and skips the terrain/chunk scan when closed. It introduces no live logging,
GPU queries, or extra scene scan. Covered-entry resource prewarming retains its
existing full-camera behavior; gameplay submission applies the exterior clip.

Portable regression coverage checks ordinary-terrain window clipping and
open/closed transitions. Installed city-group probes now assert suppression of
both terrain projections. The offline capture CSV also records existing terrain,
WDL, and WMO draw totals, and an explicit capture preload option prevents a
partially streamed scene from being mistaken for a completed visual check.

The follow-up passes formatting, all-target/all-feature Clippy with warnings
denied, and 1,359 workspace tests with 27 ignored. The installed Orgrimmar
probe also passed explicitly. Paired offline Vulkan replays use Build 127
(`ec88379d`, with only the same diagnostic preload/count additions) and the
candidate, both with all 49 terrain tiles resident. They start at Soap's saved
server position `(1464.85, -4421.25, 25.4626)`, travel by `(45, 10, 0)`, and
use camera distance 8.480558, pitch 0.06986, yaw 0.22892, realm hour 4,
1280x720, and the Testing graphics CVars including farclip 1277.

In `stationary-0000`, ordinary terrain draws change from 602 to zero while
WDL remains zero and WMO draws remain 291. In `travel_out-0014`, ordinary
terrain draws change from 603 to zero while WDL remains zero and WMO draws
remain 322. The baseline images reproduce the extra silhouettes and bare
ridges behind the towers and zeppelin; the candidate images remove them while
retaining the city. The candidate orbit also admits exterior terrain again
(67 terrain draws and 11 WDL draws at orbit frame 12).

Evidence is retained locally under `target/orgrimmar-preloaded-before*` and
`target/orgrimmar-preloaded-after*`, with stock recovery in
`target/orgrimmar-terrain-admission.c`. These use an offline fixture character
at saved coordinates, not Soap's exact latest live camera or server population.
They establish the reproduced visual correction and submission difference,
not a gameplay FPS change. Compilation/tests overlapped capture runs, so their
timing columns must not be used for performance comparison.

Testing Build 128 packages revision
`8f9b8335f6f1ebd00d461632f6f21ef16ef0729e`. The installed and sealed
executables share SHA-256
`1b0ebc905dbcf247bab91407c68221f8419a65ec81cb2e6a6e25ce2fad253118`.
The dirty identity records the build-number reservation. Installation verified
the runtime DLL hashes and Testing desktop shortcut and preserved a local
Build 127 backup.

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

## Build 128 intermittent skyline investigation

The September 13 report remains unresolved. An offline 96-step camera orbit
at saved Soap position `(1515.34, -4417.27, 18.0499)`, initial yaw `0.190609`,
distance `8.480558`, and diagnostic upward pitch `-0.25408655` reproduces a
ridge behind the city skyline. The replay preloads 49 tiles at 1280x720.
It uses the offline human character, not Soap's blood elf model; this is a
layer-isolation reproduction, not a matched live stock or FPS comparison.

At orbit frame 6, camera registration remains placement 165042, group 132.
Its exterior window is approximately `[-0.8683223, -0.38903737,
-0.62106085, 0.4860338]` in rendering min-X/min-Y/max-X/max-Y order.
Temporarily omitting WDL removes the ridge while preserving the ordinary
terrain and city draws. The temporary layer switch was removed from source;
no replacement Testing package was installed. Local captures are
`target/orgrimmar-city-gap-up/orbit-0006.ppm` and
`target/orgrimmar-city-gap-no-wdl/orbit-0006.ppm`.

An original-code replay of `7AC060` and `7A8F20` over all 144 installed groups
and 157 portals opens essentially the same exterior window at the captured
eye and reconstructed view basis. This bounds the primary portal decision,
not the complete native scene. Optional occlusion and graphics consumers
are excluded from that replay. The ordinary native WDL frustum test also
admits tiles in this direction; it does not establish complete `7CC0B0`
selection or final stock pixels.

Two potential stock occluder sources were inspected and do not explain this
specific root: the 62 static records visited by `7CD850` at `AF0040` contain
no map-1 entries, and `7D82E0` only invokes the WMO horizon-edge builder
`7D81C0` for groups named `antiportal` (string at `A405DC`). The installed
Orgrimmar root has no such named group. Terrain horizon rejection and native
render-state/order remain open comparisons. Local decompilations and native
replays use the pinned executable fingerprint recorded above and are kept
under `target/orgrimmar-*` and `target/ogrimmar-*-native.*`.

Explicit offline captures now log the camera basis, projection range,
registration, and retained exterior window after presentation. Set
`SOLARITY_WORLD_CAPTURE_PHASE=orbit` together with
`SOLARITY_WORLD_CAPTURE_DIR` to capture every orbit step. Other phases retain
their normal sparse captures. The extra registration query and diagnostic
logging run only for explicitly requested offline captures, outside measured
presentation; normal gameplay gains no query, logging, or environment lookup.
Capture runs themselves are not performance measurements.

Diagnostic validation: workspace Clippy with warnings denied, formatting check,
and all 1,359 workspace tests passed; 27 installed-data/manual tests were ignored.

## Exterior coverage of admitted horizon tiles

The remaining reproduction identifies a coverage leak after tile admission.
At the same orbit-frame-6 eye, the true-exterior opening occupies approximately
pixels X 84 through 242 at 1280 by 720. Kalimdor WDL tile `(39, 28)` intersects
that opening, but its coarse mesh also covers the central sky gap around
pixels `(555, 270)` and `(605, 260)`. Drawing the entire accepted tile puts
mountains into that gap. The captured effective camera far plane is
791.666687, after the legacy-map clamp on the configured 1277 request.

The horizon frame now retains its exterior window through command recording.
Tile-frustum tests still reject whole tiles cheaply; the WDL scissor also
restricts surviving tiles to the admitted opening. Edge conversion follows
the already recovered `6A38D0` backbuffer rounding and intersects the ordinary
world scissor. Projection stays unchanged. The original scissor is restored
before ordinary terrain, WMO, and M2 draws. This adds constant work and two
scissor commands to an admitted horizon pass, with no allocation, additional
scene traversal, query, or gameplay diagnostics. Outdoor full-window coverage
and closed-bank suppression retain their existing behavior.

The pinned native replay establishes the exterior window and conservative
tile admission. It does not establish this scissor as stock's implementation
of the final coverage restriction: the correction enforces the reported
stock visual requirement at the Vulkan submission boundary. Full native
terrain-horizon and portal-background composition remain separate research
gaps. A local terrain-edge probe through `78F900` and `78FDC0` still admitted
the offending tile, so that incomplete occlusion comparison was not used as
the fix.

The framebuffer regression alternates full, left, and right exterior windows
over the same coarse tile and frame slots. It independently checks ray/plane
coverage inside each opening and baseline sky pixels outside it, alongside
face-bank culling, extended distance, and ordinary-world depth coverage.

The corrected installed-archive replay completed seven 96-frame phases,
including captures of every orbit frame, without renderer warnings or errors.
The ridge is absent from the inspected frame 6 and adjacent skyline views
4 through 12, with ordinary city geometry and sky retained. Additional
inspected views 17, 19, 21, 23, 25, 37, and 41 cover the camera sweep away
from that gap. Local evidence is under `target/orgrimmar-city-clip/`, with
the prior-code comparison under `target/orgrimmar-city-current/`. These use
the same offline scene and camera inputs; they do not establish a matched
live Soap comparison or an FPS result.

Validation passed all 1,359 workspace tests with 27 ignored, all-target and
all-feature Clippy with warnings denied, and the workspace formatting check.
