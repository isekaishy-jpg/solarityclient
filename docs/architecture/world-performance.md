# Offline World performance replay

`solarity-runtime`'s `benchmark_world` example exercises the production terrain
streaming, environment, local-player presentation, FrameXML, camera collision,
M2/WMO presentation, and Vulkan owners without authenticating or connecting to a
realm. The caller provides a map and position. The example supplies a level-one
human warrior with empty equipment and a fixed realm clock (noon by default).

The command takes `frames-per-phase output.csv map x y z` followed by the ordinary
runtime configuration arguments. Diagnostic options `--camera-distance`,
`--camera-pitch`/`--camera-yaw` (radians), and `--realm-hour` select the recorded scene's view
and lighting. Use an isolated `--profile-root` and explicitly
select window dimensions, presentation mode, and GPU. CSV rows retain each
measured frame, including terrain publication and hover stalls. Required assets,
FrameXML callbacks, and recoverable presentation failures terminate the replay;
they do not become skipped samples.

The four phases request initial streaming, an unchanged view, a full camera
orbit, and pointer motion through FrameXML. Streaming remains active in every
phase, so the `resident_tiles` column must be inspected before describing an
interval as settled. An optional `--travel-offset dx dy dz` adds outbound,
return, and settled phases. Travel updates the authoritative fixture transform,
promotes the primary ADT through the normal asynchronous coordinator, and
submits camera-driven neighbor demand. It samples each segment in equal frame
steps, making the route independent of rendering throughput. This is a
controlled residency workload, not a real-time movement-speed measurement;
choose explicit heights appropriate to the route because no movement solver
adjusts them to terrain.

The CSV includes world coordinates and counts of ADTs admitted and evicted
during each streaming transaction. Inspect those counts alongside frame-time
outliers to distinguish steady rendering from resource publication and
retirement. Returning along the same route exercises recently departed tiles.
`ground_detail_draws` records the accepted grass/detail texture buckets.
`primary_shadow_draws` records the unit material packets submitted to the
[primary shadow map](world-shadows.md), including admitted offscreen casters.
The phase names describe input, not a promise that resource
loading has finished. Samples separate service, streaming, UI, final camera
resolution, and presentation. Streaming currently includes a camera resolution
of its own. `SOLARITY_FRAME_TIMINGS=1` additionally reports the renderer's resource,
buffer/fence, image acquisition, command-recording, submit, and present intervals.
It also aggregates camera resolutions by terrain, placed WMO, placed M2, and
terrain/WMO liquid provider. Each resolution retains all nine obstruction probes.
Residency changes additionally report terrain/liquid publication, static M2 and
WMO membership updates, and resource retirement. Per-tile upload scopes separate
geometry, blend/shadow atlases, diffuse images, and descriptor/pipeline preparation.
M2 publication scopes additionally separate requested-owner collection, retained
source lookup, new source/placement admission, placement retirement, append, and
source compaction. Changed placement topology reports effect ordering, attachment
membership, and visibility rebuilding, including model admission/sort metadata.
Per-source scopes distinguish image and mesh uploads, pipelines/samplers,
descriptors, and effects.

`SOLARITY_WORLD_CAPTURE_DIR` requests PPM framebuffer captures at the start/end
of each phase and at quarter turns of the orbit or travel segments. This explicitly waits for GPU
readback and writes images between frames, affecting both retirement and animation
time. Use a separate run without this setting for timing comparisons.

This replay does not exercise the network, movement solver, remote population,
audio, or diagnostic overlays. Its frame times are evidence about the exercised
production paths, not a substitute for measurements from a populated live world.

## Travel baseline

On 2026-09-09, a GTX 1070 replay at 1280 x 720 used 2,400 frames per phase,
map 1, start `(1100, -4500, 150)`, offset `(-1600, 0, 0)`, and camera distance
25. Capture and profiling were disabled, and no compiler ran during measurement.
The explicit elevated route isolates world residency from ground movement.

Initial streaming admitted 48 neighbors around the primary ADT. Outbound and
return travel each admitted 21 tiles and evicted 21, ending with 49 resident
ADTs. Both directions had 24 frames with residency changes.

| Phase | Mean frame | p95 frame | Maximum frame | Mean streaming on changed frames |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 6.318 ms | 8.920 ms | 103.653 ms | 25.262 ms |
| Return | 6.281 ms | 8.710 ms | 33.855 ms | 18.554 ms |

The worst outbound frame admitted one tile and spent 95.966 ms in the streaming
transaction. Frames without residency changes averaged 0.177 ms outbound and
0.179 ms returning in that component. These measurements establish a publication
stall to investigate; they do not attribute all of it to GPU transfers or prove
complete scene-loading or live-world performance parity.

Terrain geometry and blend/shadow atlases now submit without host fence waits.
The same uncaptured route still admitted and evicted 21 tiles in each direction,
with 24 changed frames. Outbound changed-frame streaming averaged 21.774 ms
(29.893 ms maximum), compared with 25.262 ms before. Returning averaged
18.527 ms, effectively unchanged from 18.554 ms. Maximum total frames were
47.739 ms outbound and 40.197 ms returning. Overall phase means were 6.412 ms
and 6.515 ms, respectively, so this run does **not** demonstrate an overall
frame-rate improvement. The smaller outbound maximum is a single-run observation,
not a guarantee about worst-case loading latency.

A separate profiled run before deferral attributed roughly 8-14 ms of changed
frames to static M2 membership. Geometry and atlas preparation also include host
serialization, staging, and allocation; removing transfer waits does not remove
that work. These costs remain under investigation. Hidden GPU regression coverage
retires an ADT before its first draw, reloads it across in-flight frames with fresh
handles, rejects retired handles, and verifies the final terrain pixels.

Restoring [static scenery distance fading](scenery-distance.md) reduced the
same route's mean total frames from 6.412 to 4.498 ms outbound and 6.515 to
4.473 ms returning (30% and 31%). Both directions again had 24 changed frames,
21 admissions, and 21 evictions. Steady presentation means fell from
5.029/5.167 ms to 3.194/3.196 ms. Changed-frame streaming still averaged
19.852/16.452 ms, so the distance policy does not resolve publication stalls.
This uncaptured, unprofiled replay used the same 2,400-frame phases and default
environmentDetail 1.0; it demonstrates reduced work on this route, without
establishing populated live-world performance or a worst-case latency bound.

## Static M2 publication

Static scenery retains a compact owner set and a list of its source slots.
Publishing a changed ADT looks up each shared static source once, without
rebuilding those identities from every large animation/effect record. Source
compaction updates this list alongside placement indices; when every source is
still referenced, the identity remap requires no instance writes. Dynamic model
sources remain separate because the same decoded model can use different texture
replacements. Placement order and animation, particle, ribbon, sound, and light
lifetimes remain owned by the existing instance records.

Immutable M2 scenes are shared between the terrain coordinator and renderer
publication. Their allocation identities survive tile promotion and distinguish
reloaded generations. Publication counts placement references only in arriving
and departing scenes, and prepares new owners only from arriving scenes. Retained
scenes no longer contribute full placement scans to requested-owner collection
or new-owner discovery. Duplicate input references register one scene; owners
shared by different scenes retain separate counts. Arrivals and departures are
evaluated together, so replacing a scene does not restart a surviving owner.
Scene counts publish after new GPU owners are ready, preserving retry discovery
if resource preparation fails. The initial GPU scene is not an extra reference:
the first full publication can already exclude it.

Static retirement also gathers source references while deciding which owners
survive. Newly admitted placements contribute their references before the same
source compactor runs. This avoids a second scan of all large instance records;
dynamic retirement retains the separate collection path. An empty geometry slot
still survives while any owner references it.

The Vulkan regression publishes overlapping MDDF identities, removes an earlier
source slot, adds new owners through the remapped slot, and then removes and
reloads the static scene while a dynamic source sharing the model stays alive.
It checks draw-resource reuse, placement order, retained playback/effect clocks,
native random consumption, and static/dynamic source separation. The lifetime
rule remains build 12340's chunk-reference ownership at `0x007A50C0`.
An additional regression exchanges overlapping scene generations, duplicates and
reorders input references, reloads an ADT at the same coordinates, and removes all
references while CPU scene handles remain alive. It checks owner order, retained
effect clocks, and exact random consumption on fresh admission.

On the same 2,400-frame travel route above, with primary shadows enabled,
profiling/captures disabled, and no compiler running, the measured changes were:

| Phase | Mean streaming on changed frames before | After | Mean total frame before | After |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 20.326 ms | 17.150 ms | 4.816 ms | 4.776 ms |
| Return | 15.886 ms | 13.215 ms | 4.856 ms | 4.719 ms |

Both directions again admitted 21 tiles and evicted 21 across 24 changed frames.
Separate profiled replays reduced retained-owner/source lookup from approximately
2.8–3.7 ms per changed publication interval to 0.03–0.05 ms. Requested-owner
collection still took roughly 1.6–2.0 ms, and retirement/compaction commonly
took about 3 ms, with an isolated 24 ms sample in the profiled run.
The loading-component means fell by 16% and 17%; overall frame means changed
much less because residency changes are sparse. Maximum total frames in the
new run were 35.139 ms outbound and 28.257 ms returning. These single-run maxima
do not establish a worst-case bound. At that stage, full requested-owner
collection, placement retirement, terrain publication, and subsequent visibility
preparation still cost time; the source index alone did not eliminate stalls or establish
populated-world performance.

Gathering source references during retirement, measured separately on the same
route and with the same disabled profiling/capture settings, further reduced
changed-frame streaming means from 17.150 to 16.417 ms outbound and 13.215 to
12.623 ms returning. Total-frame means moved from 4.776 to 4.716 ms and 4.719 to
4.658 ms, respectively. Tile admission/eviction counts remained identical.
This is about a 4% reduction in the loading component and 1.3% in whole-frame
means. The outbound maximum reached 51.912 ms, so isolated stalls remain and
the average improvement is not a worst-frame guarantee.

Incremental scene references were compared on 2026-09-09 against the subsequent
placement-metadata build, using the same 2,400-frame route at 1280 x 720 on the
GTX 1070 with primary/detail shadows enabled. Profiling and captures were disabled,
and no compiler ran during either measurement.

| Travel phase | Mean changed-frame streaming before | After | Median before | After |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 16.798 ms | 12.585 ms | 15.007 ms | 11.487 ms |
| Return | 12.166 ms | 9.502 ms | 12.271 ms | 9.580 ms |

This pair reduced the loading-component means by 25% and 22%. Whole-frame means
were 3.866/3.812 ms before and 3.847/3.854 ms after: the change primarily removes
work from infrequent tile publications. Each direction retained 24 changed frames,
21 admissions, and 21 evictions; asynchronous completion shifted some admissions
between neighboring frames. Ground-detail and primary-shadow draw counts matched
frame by frame in settled, orbit, pointer, and both travel phases. The CSV does
not expose static M2 draw counts; the ownership regressions check those lifetimes.
Maximum total frames were 46.693/23.436 ms before and 26.419/35.701 ms after, so
isolated stalls remain. These offline results do not establish populated-world
performance or the requested 1,200 FPS target.

In a separate instrumented replay, changed-scene reference accounting averaged
approximately 0.08–0.21 ms per publication interval, compared with 1.6–1.8 ms
for the previous full requested-owner collection. Late travel intervals spent
0.48–0.83 ms discovering/preparing new owners, down from roughly 1.7–2.2 ms.
Placement retirement remained around 2.0–2.6 ms, and isolated source preparation
and placement append samples still reached 16.4 ms and 7.5 ms respectively.
Those remaining publication costs require further work; instrumented component
timings are separate from the unprofiled frame measurements above.

## Per-instance lighting storage

The lighting wrapper retains an optional owned allocation for spatial query
scratch, sampled floor colors, and transition history. It allocates on the first
individual lighting sample for a unit, GameObject, or WMO doodad. Ordinary terrain
doodads return without allocating this state. This reduces the inline lighting
record from 368 bytes to 8 bytes on the tested x64 build; the newly compiled
placement layout is 784 bytes. Tile retirement therefore moves less unused
state while retaining each surviving owner's existing lighting allocation.

Callback categories, transform/revision invalidation, floor sampling, and native
transition equations remain unchanged. The first sample starts the transition
clock, including when it arrives at a nonzero scene time. Runtime regressions
cover that delayed first sample, terrain-shadow transitions, disconnection,
interior floor and MODD colors in actual M2 uniforms, offscreen light sources,
and shared owner retirement.

The same x64 GTX 1070 travel replay used 2,400 frames per phase, 1280 x 720,
primary/detail shadows, and no profiling, capture, or concurrent compiler work.

| Travel phase | Changed-frame streaming before | After | Repeat after |
| --- | ---: | ---: | ---: |
| Outbound mean | 13.325 ms | 12.232 ms | 12.221 ms |
| Return mean | 9.589 ms | 9.437 ms | 9.149 ms |

The first pair's medians changed from 12.046/9.538 ms to 11.386/9.125 ms.
Whole-frame travel means were 3.850/3.827 ms before, 3.835/3.847 ms after, and
3.881/3.831 ms on the repeat: this does not establish a steady FPS improvement.
All directions retained 24 changed frames, 21 admissions, and 21 evictions.
The first pair's primary-shadow and ground-detail draw counts matched frame by
frame across stationary, orbit, pointer, travel, and settled phases. Maximum
travel frames were 36.867/20.242 ms before, 26.777/33.461 ms after, and
28.056/21.631 ms on the repeat; isolated stalls remain.

A separate profile reduced later M2 publication intervals from about 2.9–3.6 ms
to 2.4–3.4 ms. Placement retirement fell from roughly 2.0–2.6 ms to 1.6–2.0 ms.
Texture descriptor preparation still produced isolated 5–6 ms samples, while
cached mesh pipeline lookup was much cheaper on this route. These measurements
support a smaller placement footprint and less publication work, without proving
the full stall-elimination or 1,200 FPS objective.

## Resident camera bounds

Camera traces retain conservative bounds over each immutable ADT's actual chunk
positions and each append-only M2 collision scene. WMO placements retain the union
of their camera group boxes and update it atomically with placement transforms.
This union uses MOGP boxes; the separate MOHD/MOGI movement bounds cannot replace
them. Input validation, per-chunk/per-instance tests, tolerance rules, triangle
order, and all nine camera probes remain in the query path for admitted scenes.

The 2026-09-06 replay at map 1, position `(1700, -4300, 35)`, used a GTX 1070,
2560×1440 fullscreen-windowed presentation, and 600 frames per phase. Both runs
retained 56 ADTs throughout the three settled phases. The measured mean frame
times before and after scene bounds were:

| Input phase | Before | After |
| --- | ---: | ---: |
| Stationary | 12.926 ms | 8.762 ms |
| Orbit | 12.616 ms | 8.760 ms |
| Pointer | 13.734 ms | 9.786 ms |

Final camera resolution in the stationary phase fell from 2.769 ms to 0.738 ms;
the streaming phase component also benefits from its separate camera resolution.
These runs had captures disabled and profiling enabled. The initial hover rebuild
still produces a large isolated stall, and this result does not establish the
requested overall FPS target or performance under live movement/network load.

## Tooltip text measurement

Size, spacing, and shadow-offset writes now queue individual FontStrings for
automatic extent updates. Initial loading and unindexed shared-Font writes retain
the complete pass. Both full snapshots and retained journal publication consume
the same queue; failed measurement leaves it pending.

In the same installed 1440p replay, retained tooltip copy/measurement fell from
roughly 22 ms to 0.06?0.69 ms. Geometry and glyph publication still cost several
milliseconds, and the first tooltip appearance still requires a broader rebuild.
This is a component-level result, not a claim that every hover stall is resolved.

The capture also exposed an exact-fit title wrapping onto two lines because
screen-coordinate subtraction narrowed its measured field by about 1e-13 UI
units. The shared wrapping routine now allows 1e-7 UI units of roundoff, with a
regression proving that a real 1/64-unit deficit still wraps. This applies equally
to word boundaries and enabled non-space wrapping.

## Compact M2 visibility

Terrain-owned MDDF/MODD instances retain their world-space bounding spheres in a
compact array separate from animation, particle, and material state. Each frame
tests that array before touching the much larger instance records. Placement
publication, retirement, and compaction rebuild the array in placement order.
Camera motion still tests every bound against the current frustum.

The same rebuild now records each model's light presence, parent-inherited
distance-sort flag, and the first effect's placement index. This avoids scanning
large placement records for the effect boundary and reading every model source
before the ordinary visibility test on every frame. The metadata rebuild shares
the parent lookup previously repeated by the separate distance-sort pass.
Model replacement and effect publication invalidate these values alongside the
existing bounds. Offscreen light owners still run their ordinary animation and
light updates; light visibility and sampled colors are not cached here.

Replicated WMO doodads, units, and animated attachments retain their live
transform and visibility paths. Unit animation completion runs for every dynamic
owner before visibility rejection, including offscreen units. Parent/child order,
playback clocks, particle simulation, material sampling, and transparency order
remain unchanged. The runtime regression covers camera changes, removal that
rebases placement indices, and dynamic motion after the cache has been built;
the existing unit and WMO tests cover offscreen completion and moving parents.

On 2026-09-06, the same Drag scene at 2560 x 1440 with 1,800 frames per phase,
captures and profiling disabled, measured the following mean frame times. No
compiler workload ran during either replay.

| Input phase | Before compact bounds | After compact bounds |
| --- | ---: | ---: |
| Stationary | 9.304 ms | 7.517 ms |
| Orbit | 9.105 ms | 7.362 ms |
| Pointer | 9.559 ms | 7.967 ms |

This is a 17–19% reduction in the settled phase means, approximately 126–136 FPS
after the change. It does not establish the requested 1,200 FPS target. Temporary
stage instrumentation before the change counted 28,077 resident placements and
roughly 862–985 visible instances; bone sampling itself took about 0.35 ms, while
the uncached placement scan and culling accounted for a larger share of the M2
preparation cost. That temporary instrumentation is not in the runtime.

On 2026-09-09, the metadata cache was compared on the 1280 x 720 GTX 1070
travel route above with 2,400 frames per phase and primary/detail shadows
enabled. Profiling and capture were disabled, with no compiler workload.

| Travel phase | Before metadata cache | After | Repeat after |
| --- | ---: | ---: | ---: |
| Outbound mean frame | 4.582 ms | 3.843 ms | 3.838 ms |
| Return mean frame | 4.575 ms | 3.958 ms | 3.821 ms |

The first comparison reduced mean frame time by 16% outbound and 13% returning.
Detail and primary-shadow draw counts matched frame by frame throughout the
settled, orbit, pointer, and travel phases. All travel runs retained 24 changed
frames with 21 admissions and 21 evictions in each direction. Existing offscreen
light coverage and explicitly enabled installed-archive effect tests verify
that culling still publishes light sources and newly admitted effects drain.

Changed-frame streaming means were 16.231/12.352 ms before, 15.392/14.615 ms
after, and 15.389/12.138 ms on the repeat. The first new return run contained a
49.154 ms total frame, including 42.431 ms in streaming; the repeat return
maximum was 23.503 ms. These measurements support less steady rendering work,
not elimination of publication stalls or a worst-case latency guarantee. The
offline, frame-driven route also does not establish live populated-world FPS
or the requested 1,200 FPS target.

## World graphics bindings

World command recording retains the currently bound graphics pipeline, vertex
buffer and offset, and index buffer, offset, and width across the terrain, WMO,
M2, particle, and ribbon streams. It emits a new bind only when that state changes.
The cache starts empty for every command buffer recording and ends before glow
and UI composition. Draw order, draw calls, material descriptors, dynamic uniform
offsets, and push constants retain their existing per-draw paths.

The same uncaptured 1,800-frame-per-phase replay measured 7.269 ms stationary,
7.074 ms orbit, and 7.621 ms pointer means after this change, compared with
7.517, 7.362, and 7.967 ms respectively with compact visibility alone. The
additional reduction is about 3–4%; these measurements still have the offline
replay limitations described above.

A separate profiled capture attributed about 1.04–1.13 ms to command recording,
down from approximately 1.40 ms before the binding cache. These capture timings
are component diagnostics; the frame means above come from uncaptured runs.
