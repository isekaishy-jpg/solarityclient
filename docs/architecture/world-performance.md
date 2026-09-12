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
`primary_shadow_draws` records unit and scenery packets submitted to the
[primary shadow map](world-shadows.md), including admitted offscreen casters.
`environment_shadow_near`, `environment_shadow_middle`, and `environment_shadow_far`
record actual packets in each environment update; retained maps require no
packets on frames when their cache is idle.
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

Static retirement uses the changed scene counts to identify owners whose last
reference left. When that set is empty, publication marks source use directly;
otherwise it retains placements against the departing-owner set and gathers
their source references in that pass. Newly admitted placements contribute their
references before the same source compactor runs. Dynamic retirement retains the
separate collection path. An empty geometry slot still survives while any owner
references it.

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

## M2 owner retirement

Published scene-reference changes also provide the retiring-owner set. Ordinary
updates no longer probe every placement against the full retained reference map.
The first publication still reconciles initially prepared owners with the complete
incoming scene set, since an owner from the initial GPU scene may already be
absent. All new owners prepare before these references publish. Shared owners
retain their order, playback, effects, and random state across scene changes.

The existing placement-metadata rebuild also records source slots in a compact
array. When no owner retires and that metadata is current, publication marks
source liveness from the array. Pending topology changes use the live placement
records until metadata is rebuilt. New owners contribute their source slots
separately before joining the placement vector. Source compaction invalidates
the cached slots whenever it removes entries.

This liveness pass includes dynamic placements and intentionally empty geometry.
The regression includes an unreferenced prepared source beside a live dynamic
empty mesh, and rebases source slots by compacting an earlier hole. It verifies
that later publication retains the live sources with both current and invalidated
metadata.

A temporary two-pass diagnostic separated ownership/source decisions from
record destruction and compaction on the 1280 x 720 travel route. Of 96
publications, 90 removed no owner. In the last 48 travel publications, the
decision pass averaged 1.832 ms and record compaction averaged 0.105 ms.
The six publications with removals moved an average 6.845 MB of retained
placement records; their decision/compaction means were 2.067/0.756 ms.
The decision pass also allocated retention flags and warmed record memory, so
these diagnostic times attribute the work rather than establish a frame-time
comparison. The temporary split and instrumentation were removed.

The final ordinary profiler reported 89 publications before and 88 after across
seven intervals each. Weighted mean placement retirement fell from 1.403 to
0.271 ms (81%); complete M2 residency publication fell from 3.356 to 2.235 ms.
The maximum retirement sample was 3.889 ms before and 3.984 ms after, so actual
owner destruction and compaction can still produce individual spikes.

The final unprofiled comparison ran the new version first and the preceding
build second, at 1280 x 720 with 2,400 frames per phase, primary/detail shadows
enabled, and no capture or compiler workload. Both versions admitted and evicted
21 tiles per direction. The baseline had 24 changed frames in each direction;
the new version had 24 outbound and 23 returning because asynchronous admission
coincided with another change in one frame. Detail and primary-shadow draw
counts matched frame by frame throughout all six measured phases.

| Measurement | Outbound before | After | Return before | After |
| --- | ---: | ---: | ---: | ---: |
| Mean total frame | 3.790 ms | 3.804 ms | 3.778 ms | 3.807 ms |
| Changed-frame streaming mean | 11.479 ms | 9.967 ms | 8.514 ms | 7.170 ms |
| Changed-frame streaming median | 10.989 ms | 9.261 ms | 8.208 ms | 6.855 ms |
| Total changed-frame streaming | 275.496 ms | 239.202 ms | 204.328 ms | 164.911 ms |
| Maximum total frame | 24.285 ms | 22.130 ms | 20.896 ms | 22.848 ms |

The loading-component reduction does not establish an FPS gain: the overall
means above were slightly higher, as were the stationary, orbit, pointer, and
settled means in this pair. Settled mean frame time was 4.863 ms before and
4.892 ms after. These offline measurements establish neither a latency bound
nor populated-world performance or the requested 1,200 FPS target.

## Dynamic M2 state lookup

Settled frame preparation now locates unit bodies and mounts through the compact
placement metadata instead of searching every static scenery instance. The
metadata records the first index for each dynamic owner and a separate ordered
list of GameObject roots and replicated WMO doodads. The hash table is used only
for lookup; traversal and state updates retain their previous order. Static
owners do not occupy the table.

Placement publication, retirement, source compaction, and effect publication
invalidate these indices with the existing topology flag. Before the next draw
preparation rebuild, state updates search current placement records. Missing
unit owners retain their existing errors. Missing GameObject input still
invalidates resident roots and doodads, including offscreen owners; an empty
input does not skip this work. This changes candidate lookup, without changing
stock transforms, unit synchronization, playback, random draws, or culling.

Existing mounted-unit regressions exercise all six body/mount owner categories
with rebuilt metadata and with indices dirtied by dismount publication. The
GameObject and WMO regressions cover moving parents, invalid placement, retained
playback/random state, and empty CPU input before GPU retirement.

On 2026-09-09, the same 1280 x 720 GTX 1070 route ran for 2,400 frames per
phase, first with Build 69 functionality and then with this change. Primary and
detail shadows were enabled; profiling, capture, and compiler activity were
absent during both runs.

| Input phase | Before | After |
| --- | ---: | ---: |
| Stationary mean frame | 4.845 ms | 4.624 ms |
| Orbit mean frame | 4.024 ms | 3.790 ms |
| Pointer mean frame | 5.224 ms | 5.001 ms |
| Outbound mean frame | 3.815 ms | 3.586 ms |
| Return mean frame | 3.803 ms | 3.561 ms |
| Settled mean frame | 4.965 ms | 4.718 ms |

These phase means fell by 4.3-6.4%. Detail and primary-shadow draw counts matched
frame by frame in all six phases. Both runs had 24 changed frames, 21 admissions,
and 21 evictions in each travel direction. Changed-frame streaming means were
10.717/7.109 ms before and 9.737/7.013 ms after; this lookup change does not remove
publication work. Travel maximum total frames were 39.902/28.506 ms before and
23.033/17.704 ms after. These isolated maxima are not latency bounds.

A separate production-profile run compared with the preceding Build 69 profile
places the main saving in object-state preparation: the final two settled
interval means fell from 191.7-193.8 microseconds to 2.85-3.09 microseconds. The
broader unit-state stage still costs 189.4-192.6 microseconds (previously
212.8-213.2); it includes sky updates and cannot be attributed entirely to unit
lookup. Reported topology rebuild means were 1.561 ms for 89 changes before and
1.585 ms for 90 changes after. Metadata rebuilds and dirty-frame full searches
remain necessary. This offline result does not establish populated-world FPS,
complete stall elimination, or the requested 1,200 FPS target.

## Unit effect retirement

The M2 preparation profile separates scene setup, residency/topology work,
dynamic models, instance traversal, transparent ordering, and scene lighting.
It identified a full placement scan in effect retirement even when a settled
scene had no unit effects. Current metadata already records the first effect;
retirement now extracts drained effects only from that candidate range and
keeps all surviving placements in order. Dirty topology uses the full current
list until its boundary is rebuilt. Attachment cleanup uses the surviving
candidate range, and source compaction still follows an actual removal.

The original retirement phase and live-particle predicate are unchanged.
Focused coverage includes current/dirty boundaries, surviving ordinary models
on both sides of an effect, completion timers/random draws, same-frame callback
publication, attachment replacement, and offscreen particle draining. The two
installed-archive checks also pass, including all twelve instances from nine
GPU effect sources.

On 2026-09-09, final settled profile intervals on the same 1280 x 720 GTX 1070
route measured the residency/topology stage at 190.839 us before and 0.261 us
after (852 and 889 frames). Total M2 preparation measured 1.559 and 1.372 ms.
These intervals have current metadata; changed frames still rebuild topology.

Separate 2,400-frame-per-phase replays, with profiling and capture disabled
and no compiler workload, measured these total frame means:

| Phase | Before | After |
| --- | ---: | ---: |
| Stationary | 4.555 ms | 4.394 ms |
| Orbit | 3.753 ms | 3.549 ms |
| Pointer | 4.955 ms | 4.812 ms |
| Outbound travel | 3.558 ms | 3.388 ms |
| Return travel | 3.541 ms | 3.400 ms |
| Settled | 4.660 ms | 4.457 ms |

The means fell by 2.9-5.5%. Detail and primary-shadow draw counts match frame
by frame in all six phases. Each direction retains 24 changed frames, 21
admissions, and 21 evictions. Return maximum frame time was 22.834 ms before
and 24.490 ms after; this is not evidence of stall elimination or a latency
bound. The offline route does not establish populated-world performance or
the requested 1,200 FPS target.

## Procedural cloud noise

The sky profile separates gradient colors, celestials, cloud lighting, and
procedural texture rows. Scene preparation also separates sky and uniform
construction from unit state updates. These scopes use the existing opt-in
frame timing path and make no clock calls when profiling is disabled.

Cloud noise shares Y/Z interpolation state across each row and corner values
across adjacent columns in the same X cell. The original interpolation order,
f32 corner differences, eight-row update schedule, lighting, and bank switches
remain intact. The [native cloud oracle](world-sky.md) now checks 44 updates,
including the 16-bit phase wrap; every earlier fixture record is unchanged.

On 2026-09-09, two complete 2,400-frame-per-phase profiles on the 1280 x 720
GTX 1070 travel route above measured 167.645 us before and 147.648 us after for
cloud rows in their final two settled intervals (853 and 855 frames). Total
sky update measured 186.007 and 166.267 us. This is approximately 12% less
cloud work, or 20 us per frame in that sample.

Separate replays with profiling and capture disabled, and no compiler workload,
measured these total frame means:

| Phase | Before | After |
| --- | ---: | ---: |
| Stationary | 4.652 ms | 4.541 ms |
| Orbit | 3.805 ms | 3.737 ms |
| Pointer | 5.055 ms | 5.054 ms |
| Outbound travel | 3.584 ms | 3.572 ms |
| Return travel | 3.544 ms | 3.555 ms |
| Settled | 4.650 ms | 4.640 ms |

The total-frame changes are small and mixed; the component timing establishes
the cloud saving. Detail and primary-shadow draw counts match frame by frame
in all six phases. Each direction still has 24 changed frames, 21 admissions,
and 21 evictions. Return travel maximum frame time increased from 21.018 to
24.339 ms in this pair. This does not establish reduced publication stalls,
live populated-world performance, or the requested 1,200 FPS target.

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

## M2 texture descriptor allocation

Material batches allocate their unique new descriptor sets from the latest pool
while it has capacity. A new pool grows from the previous capacity and current
batch demand, amortizing driver allocation across models. Each set still charges
two combined image/sampler descriptors for the common two-binding layout. The
allocator reserves spare capacity but adds no fixed material or residency limit.

Pools retain every successful allocation until renderer teardown, after all draws
retire. Image/sampler identity, material-stage ordering, and cached handles keep
their existing rules. The Vulkan allocation counter advances only after a full
batch succeeds; a failed first allocation destroys its unused pool. No sets are
freed individually. These rules follow the Vulkan guarantees for
[pool fragmentation](https://docs.vulkan.org/refpages/latest/refpages/source/VkDescriptorPoolCreateInfo.html)
and [atomic descriptor allocation failure](https://docs.vulkan.org/refpages/latest/refpages/source/vkAllocateDescriptorSets.html).

The regression mixes one- and two-stage materials across successive batches,
includes duplicates and empty requests, and renders earlier descriptors after
pool growth. A separate framebuffer regression grows later pools before checking
the original particle texture's expected fogged pixel values.

On 2026-09-09, temporary instrumentation attributed 27.427 ms of 29.304 ms across
715 new material batches to descriptor allocation. Pool creation, descriptor
writes, and registry publication totaled 0.874, 0.288, and 0.378 ms respectively.
The largest allocation was 6.658 ms for a single new set without registry growth.
This instrumentation was removed before the optimized replay.

Separate full travel profiles each reported 1,724 M2 source publications. Mean
texture-descriptor preparation fell from 20.400 to 2.062 microseconds, and the
maximum fell from 5.504 to 0.424 ms after sharing pool capacity. These are component
measurements; other resource uploads and scene preparation still take time.

The unprofiled comparison used the same 2,400-frame travel route, GTX 1070,
1280 x 720 extent, and primary/detail shadows, with captures and compilers absent.

| Travel phase | Mean changed-frame streaming before | After | Mean total frame before | After |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 13.048 ms | 12.306 ms | 3.833 ms | 3.782 ms |
| Return | 9.200 ms | 9.098 ms | 3.868 ms | 3.747 ms |

Both directions retained 24 changed frames, 21 admissions, and 21 evictions.
Streaming medians were 11.452/9.216 ms before and 11.629/8.974 ms after; the
outbound median did not improve. Primary-shadow and ground-detail draw counts
matched frame by frame across stationary, orbit, pointer, travel, and settled
phases. Maximum travel frames changed from 39.702/43.679 ms to 27.185/20.214 ms,
but these single-run maxima are not latency bounds, and the entire difference
cannot be attributed to descriptor allocation. The component profile establishes
less descriptor work; this pair does not prove elimination of all loading stalls,
populated-world performance, or the 1,200 FPS target.

## Mesh upload serialization and staging

Mesh uploads copy the borrowed vertex and index payloads directly into their
aligned ranges in mapped staging memory. The previous intermediate combined
CPU byte vector is no longer allocated, filled, and freed for each mesh. The
shared path serves terrain, M2, liquids, WMO, and static UI geometry. Transfer
padding is still zeroed, logical byte counts are unchanged, and the same flush,
queue barriers, submission, and fence-owned retirement remain in place. The
[VMA mapping contract](https://gpuopen-librariesandsdks.github.io/VulkanMemoryAllocator/html/memory_mapping.html)
requires flushing noncoherent writes and favors copies that do not read mapped
memory; both properties are retained.

Terrain serialization assembles each 52-byte vertex on the stack and appends
it once, instead of appending thirteen separate float components. Positions,
normals, texture coordinates, and RGB remain explicitly little-endian. Authored
MCCV remains raw optional BGRA in the CPU vertex; its GPU RGB conversion still
normalizes authored bytes and uses exactly 0.5 when MCCV is absent.

A temporary diagnostic on the same 1280 x 720 travel route measured 91 terrain
uploads, including 49 initial tiles and 42 travel admissions. The mean timed
interval through transfer submission was 1.929 ms: encoding accounted for
0.888 ms, constructing the combined CPU vector for 0.473 ms, staging
allocation/write and submission-object creation
for 0.232 ms, device-buffer allocation for 0.024 ms, command recording for
0.042 ms, and submission for 0.044 ms. Fence retirement averaged 0.00016 ms.
These nested measurements omit combined-vector cleanup from the component sum;
registry publication and source-vector cleanup follow the outer timed interval.
The diagnostic was removed before validation and comparative benchmarking.

Existing rendering regressions exercise the shared path through terrain pixel
oracles with authored and absent MCCV, live terrain retirement/reload, M2
geometry with a six-byte logical index buffer, and WMO/UI/liquid rendering.

The ordinary frame profiler reported 85 tile uploads across eight intervals
in each version. Weighted mean geometry upload fell from 2.056 to 1.273 ms
(38%); total tile upload fell from 2.940 to 2.174 ms. The largest geometry
sample was 5.055 ms before and 5.228 ms after, so this mean reduction does not
establish elimination of individual upload spikes.

The 2026-09-09 paired travel replay used 2,400 frames per phase, the GTX 1070,
primary and detail shadows enabled, and no capture, profiling, or compiler
workload. Both versions admitted and evicted 21 tiles in each direction across
24 changed frames. Detail and primary-shadow draw counts matched frame by frame
in the stationary, orbit, pointer, outbound, return, and settled phases.

| Measurement | Before | After |
| --- | ---: | ---: |
| Outbound mean frame | 3.802 ms | 3.771 ms |
| Return mean frame | 3.751 ms | 3.742 ms |
| Outbound changed-frame streaming mean | 13.086 ms | 11.388 ms |
| Return changed-frame streaming mean | 9.278 ms | 8.384 ms |
| Outbound changed-frame streaming median | 11.040 ms | 10.539 ms |
| Return changed-frame streaming median | 9.322 ms | 8.500 ms |
| Outbound maximum frame | 44.309 ms | 24.328 ms |
| Return maximum frame | 20.308 ms | 19.733 ms |

This reduces admission work; settled/orbit/pointer means do not show a consistent
FPS improvement. The baseline outbound maximum includes a 38.710 ms streaming
sample, so the change in maxima and means is not wholly attributable to this
optimization. These runs establish neither a latency bound nor populated-world
performance or the requested 1,200 FPS target.

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

## Portal-aware WMO surface submission

Runtime surface drawing now consumes the scene's native ordered WMO group
callbacks and their local portal frusta. It selects each group's first-accepted
resident batches instead of testing every batch against the full camera.
[The visibility evidence](world-model-batch-visibility.md) includes native
selection loops, mixed static/moving depth lists, a Vulkan packet/pixel test,
and the installed Orgrimmar graph at the captured benchmark camera.

On 2026-09-10, Build 73 and this change were compared on the same 1280 x 720
GTX 1070 travel route, with 2,400 frames per phase, shadows enabled, profiling
and capture disabled, and no compiler running. The local playerbots server
was running during both offline replays.

| Input phase | Build 73 mean frame | Portal submission |
| --- | ---: | ---: |
| Stationary | 4.540 ms | 4.043 ms |
| Orbit | 3.692 ms | 3.609 ms |
| Pointer | 4.917 ms | 4.484 ms |
| Travel outbound | 3.563 ms | 3.472 ms |
| Travel return | 3.524 ms | 3.425 ms |
| Settled after travel | 4.628 ms | 4.175 ms |

The stationary mean fell about 11%, and travel means about 3%. Detail and
primary-shadow counts matched frame by frame in all six phases. Both travel
directions retained 24 changed frames, 21 admissions, and 21 evictions. This
is a bounded offline comparison, not live populated-world FPS or evidence
that the requested 1,200 FPS target has been reached.

Separate captures exposed a changed Orgrimmar silhouette. Replaying the
installed model through original portal projection, recursion, local clipping,
and batch selection confirmed the new group and batch decisions before
accepting the measurement. Per-group fog shaders, liquids, and attached
doodads still have separate integration gaps described in the evidence.

## Ribbon material and retained-fog replay

On 2026-09-12, Build 103 includes the recent model/effect opacity, registered
owner fog, ribbon color/culling, grouped material order and retained-fog changes.
The ordinary ribbon shadow audit confirms that its native element selects no
shadow receiver. This build preceded the surface-fog follow-up below.

Two uncaptured replays of each optimized executable used the same 1280 x 720
GTX 1070, uncapped presentation, shadow quality 2, noon clock and 2,400 frames
per phase. The route starts at map 1 `(1100, -4500, 150)`, travels 1,600 yards
west and returns. Run order was current, previous, current, previous. Profiling
was disabled and no compiler ran during these four measurements. The previous
executable was preserved from the 2026-09-11 10:48 build, after model/liquid
clipping work; this comparison spans several correctness changes.

| Phase | Previous mean frame | Build 103 mean frame |
| --- | ---: | ---: |
| Stationary | 2.905 ms | 2.983 ms |
| Orbit | 3.139 ms | 3.266 ms |
| Pointer | 3.284 ms | 3.359 ms |
| Travel outbound | 3.266 ms | 3.334 ms |
| Travel return | 3.232 ms | 3.307 ms |
| Settled after travel | 2.965 ms | 3.013 ms |

Means combine both runs for each executable. Current frame times are about
1.6–4.0% higher. All non-loading frame positions, terrain-detail draw counts
and primary-shadow counts match across all four runs. Each direction retains
24 changed frames, 21 tile admissions and 21 evictions; each run ends with
49 resident tiles. The current runs still contain non-loading samples up to
35.651 ms. These results establish neither stall elimination nor the requested
1,200 FPS target; the phase means correspond to roughly 298–335 FPS in this
offline fixture with no authored NPCs or network population.

Separate profiled runs put final settled command recording at 408–409 µs
current versus 394–398 µs previous. M2 preparation remains around 0.70–0.71 ms
in those intervals. The uncaptured CSVs also show roughly 26–36 µs more UI time
in the selected phases despite no corresponding UI change in this slice.
The comparison does not isolate all added cost to the ribbon fog work; retain
the performance regression as an open optimization item.

Local evidence is `target/ribbon-world-{before,after}-{one,two}.csv` and the
separate `before-profile`/`after-profile` logs. Benchmark executable SHA-256:
previous `f10decaf9598d99fca5e8d6b2ceafa8b95f4d489aa4424604739698cc9f3327a`,
current `61058fb453b93930ac60fc70ac9d3f7bebf3e54049530f19bb43de51c8bc1c9c`.

## Surface fog and deferred register conversion

Build 104, installed on 2026-09-12, adds native camera-depth fog for M2/WMO,
physical WMO pass bank/enable policy, and deferred conversion of the shared
fog registers consumed by ribbons. Publishing model/particle/WMO draws now
retains value snapshots instead of calculating intermediate coefficients and
packed colors that may be replaced before any ribbon consumes them. Native
register and pixel checks cover preservation of startup, disabled and retained
state. All 1,322 workspace tests pass, with 23 environment-dependent cases
ignored, and workspace Clippy passes with warnings denied.

The same 1280 x 720 GTX 1070, shadow-quality-2, uncapped noon route was replayed
twice per executable, with 2,400 frames per phase and no compiler, profiling
or capture active. Run order was 104, 103, 104, 103. Means combine both runs:

| Phase | Build 103 mean frame | Build 104 mean frame |
| --- | ---: | ---: |
| Stationary | 2.716 ms | 2.729 ms |
| Orbit | 2.975 ms | 2.977 ms |
| Pointer | 3.081 ms | 3.091 ms |
| Travel outbound | 3.141 ms | 3.123 ms |
| Travel return | 3.087 ms | 3.051 ms |
| Settled after travel | 2.752 ms | 2.748 ms |

Changes range from 1.14% faster to 0.49% slower; these runs establish no clear
overall FPS gain. Current means correspond to about 320-366 FPS. Non-loading
positions, terrain-detail counts and primary-shadow counts match frame by
frame in all four runs. Each travel direction admits and evicts 21 tiles and
each run ends with 49 residents. Changes occupy 24 frames per direction except
one baseline return, where asynchronous completions occupy 23 frames.

The largest current non-loading sample is still 37.530 ms. In the second
current run, pointer entry spends 28.151 ms in UI work, and two outbound
admissions spend about 15 ms in streaming. Other spikes include roughly 16 ms
of UI work during stationary/orbit phases and a 19 ms presentation sample.
Those costs remain explicit stall investigations; this replay has no authored
NPC population, network, movement solver, audio or overlays, and does not
establish live-world parity or the requested 1,200 FPS.

Separate profiling runs put the final two settled command-recording intervals
at 379.4/375.7 microseconds current versus 380.9/379.5 previous. The small
difference does not establish a material end-to-end gain or isolate the whole
comparison to deferred fog conversion. M2 preparation remains about 0.66-0.67 ms.
Terrain-publication profiles also retain multi-millisecond M2/WMO membership
and terrain/liquid work. Next optimization work should target the measured
UI-entry and admission stalls rather than treating fog conversion as sufficient.

Local evidence is `target/surface-world-{before,after}-{one,two}.csv`, the
separate `before-profile`/`after-profile` logs, and
`target/compare-surface-world.py`. Benchmark SHA-256:
Build 103 `61058fb453b93930ac60fc70ac9d3f7bebf3e54049530f19bb43de51c8bc1c9c`,
Build 104 `328a5bfe6644c40b2831c04a1f5205fa94b3a8b891be44ce893b0a05382fb244`.
The installed Build 104 runtime matches its package SHA-256
`63b548b02491c0942f7367773324e65f9338df7b132cc1498083e6812fc14a1e` and
records source revision `7970ac01` with the reserved package number.

## Mixed layout transactions and first tooltip publication

The 2026-09-12 follow-up profiles World UI clock, pointer dispatch, Lua/retained
publication, portrait, resource preparation and minimap separately under the
existing opt-in frame profiler. The periodic roughly 16 ms UI samples came from
one transaction containing a performance-bar vertex color and BuffFrame anchors
that were cleared and restored. The old unchanged-layout check accepted only
pure layout journals, so this combination rebuilt all geometry and presentation.

Publication now removes only layout bits whose final inputs match retained
state, keeping other fields on the same object and other objects' mutations.
Automatic text measurement still prevents this pruning until its pending layout
work is applied. The resulting color-only transaction uses the existing vertex
patch. Authored callbacks still observe their intermediate anchor changes.
The first tooltip reveal also defers missing visual topology to the content
publisher, avoiding a build with old text/geometry before the actual publication.
Regression coverage checks mixed texture/color writes, real layout changes,
pointer behavior and a hidden tooltip resized during its first reveal.
All 1,322 workspace tests pass, with 23 environment-dependent cases ignored;
workspace Clippy passes with warnings denied, as does the formatting check.

Two unprofiled, uncaptured runs per executable used the same 1280 x 720 GTX 1070,
shadow-quality-2 uncapped noon route and 2,400 frames per phase as Build 104.
Run order was current, preserved 104, current, preserved 104; no compiler or
tests ran during measurement. Means combine both runs:

| Phase | Build 104 mean frame | Current mean frame |
| --- | ---: | ---: |
| Stationary | 2.714 ms | 2.682 ms |
| Orbit | 2.971 ms | 2.930 ms |
| Pointer | 3.077 ms | 3.040 ms |
| Travel outbound | 3.118 ms | 3.062 ms |
| Travel return | 3.049 ms | 2.997 ms |
| Settled after travel | 2.743 ms | 2.705 ms |

Mean improvements range from 1.18% to 1.78%, or about 327-373 FPS current.
UI means fall by 37-45 microseconds. Baseline stationary/orbit UI maxima are
15.82-16.09 ms, versus 0.86-1.53 ms current, with no current UI samples over
10 ms in those phases. First pointer-entry UI time falls from 28.746/28.187 ms
to 19.802/18.793 ms. A separate profiled run confirms that initial tooltip
publication copies in 0.487 ms and builds once; global geometry still costs
6.852 ms and the full targeted publication totals 17.104 ms. The initial
streaming transaction still publishes a real BuffFrame layout change; pruning
does not suppress that work.

Non-loading positions, terrain-detail draws and primary-shadow draws match
frame by frame across all four runs. Each direction admits and evicts 21 tiles,
and all runs end at 49 residents. Changed residency occupies 24 frames per
direction except the first baseline and second current returns, which take 23.
Current non-loading frame maxima still reach 22.482 ms. Outbound admission
samples retain roughly 14-15 ms streaming work, and presentation waits can
approach 20 ms. This is a specific UI stall reduction, not stall elimination or
the requested 1,200 FPS. The fixture still has no authored NPCs, network,
movement solver, audio or overlays.

Local evidence: `target/world-stall-{before,after}-{one,two}.csv`,
`target/compare-world-stall.py`, `target/analyze-world-stall-ui.py`, and the
separate `world-stall-diagnostic.log` / `world-stall-fixed-profile.log`.
Preserved 104 benchmark SHA-256 is
`328a5bfe6644c40b2831c04a1f5205fa94b3a8b891be44ce893b0a05382fb244`;
current benchmark is
`13271ad5a941df32e3496c256fb70c4d9ce939155ee77dcea8336a5482b335e7`.

Testing Build 105 packages this change from source revision `cc674a35`.
Its installed executable matches the package SHA-256
`749f18aacb06f2f3e679ad1f67b7fe271741e7c2caf39c4490c106ba3a664434`;
the recorded dirty state is the reserved package number. The measured benchmark
is also preserved as `target/benchmark-before-terrain-admission.exe` for the
next comparison. Follow-up profiling should separate terrain serialization,
allocation, staging and submission: one current terrain geometry upload sample
alone reaches 11.039 ms inside a 12.479 ms tile upload.

## Shared liquid transfer admission

The next 2026-09-12 investigation splits terrain geometry admission into
retirement, serialization and upload, and mesh upload into allocation, staging,
recording and submission under `SOLARITY_FRAME_TIMINGS`. The earlier 11 ms
geometry outlier did not recur in the diagnostic replay. Across 91 terrain
uploads, serialization averages 763.9 microseconds and upload 317.9 microseconds;
retirement averages 0.09 microseconds. Serialization remains unchanged here.

Liquid strips previously created separate staging allocations, command pools,
command buffers, fences and queue submissions. Terrain and WMO admission now
prepare materials first and submit each group of strips through one shared
transfer. Every strip retains its own vertex/index buffers, handle, index count,
draw order, transforms, lighting and retirement lifetime. Each source payload
is padded independently to the four-byte transfer unit, with explicit barriers
covering subsequent input reads on the graphics queue. Validation happens before
allocation; pre-submission failures release partial allocations and publish no
mesh handles. The source data, shaders, animation and randomization are unchanged.

The two diagnostic replays contain exactly 3,707 logical mesh uploads each.
Previously all 3,707 were separate transfers. Current has 1,447 individual
transfers plus 74 batches covering 2,260 meshes, reducing transfer submissions
to 1,521. The batch transfer mean is 257.5 microseconds and maximum 1,235.8
microseconds; one 40-strip batch takes 192.4 microseconds. These diagnostic
figures include profiling and are distinct from the unprofiled comparison below.

GPU regression coverage uploads different vertex extents and odd/even index
counts together, checks rendered liquid blending/depth pixels across frame-slot
reuse, rejects a batch containing an invalid strip, and renders the surviving
member after independently retiring its sibling. Empty and single-mesh calls
retain their supported behavior.
All 1,322 workspace tests pass with 23 environment-dependent cases ignored;
workspace Clippy passes with warnings denied, and formatting checks pass.

Two unprofiled, uncaptured replays per executable again use the GTX 1070,
1280 x 720, shadow quality 2, uncapped noon map-1 route and 2,400 frames per
phase. Order is current, preserved 105, current, preserved 105, with no compiler
or tests running during measurement. Each row combines both runs:

| Phase | Build 105 mean frame | Batched mean frame |
| --- | ---: | ---: |
| Stationary | 2.735 ms | 2.724 ms |
| Orbit | 2.951 ms | 2.968 ms |
| Pointer | 3.078 ms | 3.061 ms |
| Travel outbound | 3.024 ms | 3.052 ms |
| Travel return | 2.989 ms | 3.036 ms |
| Settled after travel | 2.713 ms | 2.742 ms |

There is no clear overall FPS gain: changes range from 0.57% faster to 1.58%
slower, with current phase means about 327-367 FPS. UI means are 36-49
microseconds higher despite no UI source change; that regression remains an
unresolved investigation. Current non-loading samples still reach 34.460 ms,
including a 24.512 ms presentation cost combined with 9.354 ms UI work.

The 48 residency-changing frames per direction across both runs isolate the
admission gain. Outbound streaming mean falls from 9.508 to 7.759 ms and total
frame mean from 16.621 to 14.696 ms. Return streaming mean falls from 6.355 to
5.949 ms and total frame mean from 13.097 to 12.563 ms. Maxima remain variable:
outbound streaming maximum falls from 29.721 to 14.952 ms, while return maximum
rises from 8.185 to 14.349 ms. The baseline outbound outlier contributes to its
mean; these results do not establish elimination of admission stalls.

All four runs match non-loading positions, detail draws and primary-shadow
draws frame by frame. Each direction has 24 residency-changing frames per run,
21 admissions and 21 evictions, ending with 49 resident tiles. The offline
fixture's limitations remain: no authored NPC population, network, movement
solver, audio or overlays. The 1,200 FPS target is still open.

Local evidence: `target/terrain-admission-{before,after}-{one,two}.csv`,
`target/compare-terrain-admission.py`, `target/compare-admission-cost.py`,
`target/terrain-admission-diagnostic.log`, `target/terrain-admission-batch-profile.log`,
and `target/summarize-admission-profiles.py`. Benchmark SHA-256:
previous `13271ad5a941df32e3496c256fb70c4d9ce939155ee77dcea8336a5482b335e7`,
batched `53ee2b64bd940a476e1a2d2673d20f2508d1d26276da301867453248421f4226`.

Testing Build 106 packages source revision `97d264e3`. The installed runtime
matches the package SHA-256
`0cb3a1f8485d58b427d8b536aaafa2f0b292c4ee503858b5b85291b2b16f5845`,
with the reserved package number recorded as dirty source state. The measured
batched benchmark is preserved as `target/benchmark-before-ui-dispatch.exe`.
