# Performance foundation implementation

The September 13 architectural audit establishes five required workstreams.
The purpose is to make feature work inherit bounded, retained work rather than
requiring another broad performance pass as content grows. Preserve existing
stock behavior, including visibility, ordering, animation and error contracts.

## Required contracts and progress

- [x] Local UI updates and stable owner storage: scrolling avoids full snapshots
  and document-wide glyph scans; changing text length leaves unrelated geometry
  in place. Extend retained publication and partial buffer updates.
- [x] Persistent font and glyph coverage: retain exact font metrics and glyph
  rasterization across text owners; adding glyphs preserves existing coverage.
- [x] Change-driven placement and appearance: transient effects do not rebuild
  static scenery metadata; unchanged local-player inputs reuse appearance plans.
  Local-player admission and independent pose updates have separate modules.
- [x] Shared draw preparation and instancing: retain validated immutable draw
  templates and batch compatible scenery/shadows using compact instance records,
  with simulation, pose and render work requirements kept explicit.
- [x] GPU lifetime and growth: retire superseded resources after their final
  submitted use; ordinary growth does not drain the device. Account for retained
  resources and test repeated content changes for generation accumulation.

Split affected implementations into folder-backed responsibility modules. This
is not a repository-wide cleanup. Glyph instancing, broad animated-character
batching, aggressive pose sharing, indirect/bindless rendering and GPU animation
remain later work; the interfaces should permit them.

## Validation and delivery

Exercise ownership/invalidation and scaling contracts through representative
consumers and regression tests. Run required formatting, workspace Clippy and
tests. Compare Soap in matched city scenes and exercise EULA scrolling at the
same resolution and settings; report frame times and limits of the evidence.
Finish with a numbered Testing package, independently reviewable commits and
push to `perf/critical-frame-work`. Build 130 is the installed starting point.

## Completed source checkpoints

`08840c61` retains local appearance inputs and publishes effect-tail metadata
without rebuilding ordinary placement membership or the scenery index. Source
compaction remaps compact references directly. Five equipment tests, 20 related
placement tests and two effect tests passed (one manual benchmark ignored).

The UI checkpoint separates physical quad allocation from draw order, reuses
vacant spans, and updates only changed source geometry. Clipping indexes short
ordered blocks and skips offscreen document ranges. Scroll dispatch journals
name their owners; range caching follows layout/visibility changes and explicit
child-rect publication. Nineteen renderer UI tests and 161 UI integration tests
passed, including repeated growth, 20,000-line clipping and a no-full-snapshot
wheel test. UI/runtime Clippy passed with all targets/features.

`82ae6a54` shares exact font faces, metrics, kerning and rasterized glyph coverage
between measurement and rendering, scoped to the mounted archive identity.
`933a12e4` appends missing coverage into stable atlas pages. Existing glyph UVs
and texels survive new characters and text owners. Full UI rebuilds retain the
coverage bank; region uploads are queue ordered and do not wait on the host.
Twenty-eight UI unit tests and 162 integration tests passed at this checkpoint.

M2 source admission now retains validated mesh/material draw templates. Frame
preparation supplies transforms, material state and palette offsets without
revalidating immutable resource joins. Source CPU preparation, GPU admission,
and frame preparation have separate folder modules; visibility, pose, attachments,
lighting and effects retain their existing activity requirements and ordering.

Normal and shadow pipelines consume a packed 336-byte instance stream. Adjacent
compatible opaque/alpha-test draws share an indexed submission. Distinct receiver
indices may share a batch when their resolved lighting is equal. Geometry,
pipeline, texture, lighting or effect-order boundaries end the batch. This does
not globally reorder visible geometry or batch transparent effects. The GPU
regression compares 16 instances in one submission against 16 separate draws,
requiring visible geometry and identical captured pixels.

Resident model sources and prepared UI frames pin their GPU resources. Last-owner
release queues invalidation and a graphics completion fence. Retired images,
meshes and dependent descriptors remain allocated until that fence completes;
live registries remove obsolete metadata instead of accumulating tombstones.
Character descriptor pools reuse completed slots. Authored BLP path caches and
finite pipeline/sampler caches remain shared common resources. On-demand usage
reports count live resources and payload capacity, plus pending retirement batches;
the byte totals exclude retired batches and driver allocation overhead.

UI mesh growth queues an allocation transfer and retires old buffers at completion.
M2-only/portrait frame storage grows on each reused frame slot after its ordinary
fence. Unified world frame storage already used this slot-local growth policy;
it was retained. Surface/depth recreation can still require device idle.

Required formatting and workspace Clippy (all targets/features, warnings denied)
passed. The all-feature workspace suite passed 1,380 tests with 28 intentionally
ignored. This includes 192 renderer integration tests and the runtime's shadow,
portal, minimap and placement-publication consumers. Retained UI upload tests
also cover simultaneous prefix changes and appended bytes; the generic single-
source replacement contract remains separate from multi-page glyph replacement.

## Live validation

The 2560 x 1440 EULA replay uses the authored `EULAScrollFrame` wheel handler,
normal UI publication and Vulkan presentation. Each phase lasts 12 seconds after
a 20-second settling period. Captures confirm readable text and clipping at the
top and bottom; offsets go from 0 to 4,213 and back to 0. The agreement is not
accepted. The isolated test profile leaves the Testing profile unchanged.

| Candidate phase | Mean frame time | Mean FPS | p95 frame time |
| --- | ---: | ---: | ---: |
| Top, stationary | 2.267 ms | 441.0 | 2.517 ms |
| Scrolling down | 2.295 ms | 435.7 | 2.460 ms |
| Bottom, stationary | 2.293 ms | 436.2 | 2.457 ms |
| Scrolling up | 2.344 ms | 426.6 | 2.513 ms |

This candidate replay does not show the reported fourfold scrolling slowdown.
It is not a matched old/new EULA benchmark. Captures and raw output are retained
under `target/foundation-eula2.*` in the local validation workspace.

The user's movement during the first city sample makes that sample unsuitable
for FPS comparison. It is retained only as movement smoke evidence. Subsequent
runs reset Soap's location and saved camera. Stationary averages, time-related
drift and moving-scene costs are evaluated separately.

The city comparison uses Soap at `(1515.34, -4417.27, 18.0499)`, orientation
`0.190609`, camera distance `8.480558`, pitch `12.604312`, 2560 x 1440, Ultra
(`farclip=1277`, environment detail 1.5, shadows 5), VSync off. CPU frame profiling
is enabled on both builds, GPU profiling disabled. No compilation runs during
timing. Hardware is the same i5-9600K/GTX 1070 machine. The baseline diagnostic is
Build 130 source `db0a0e2e`, compiled immediately before packaging and therefore
stamped Build 129. The candidate is `aec1632f`, with test-only local-login and
resource-snapshot adapters. These adapters are removed after diagnostic builds.

| Stationary city window | Mean frame time | Mean FPS | Best two-second mean FPS |
| --- | ---: | ---: | ---: |
| Baseline, first measured minute | 8.780 ms | 113.9 | 122.4 |
| Candidate, first measured minute | 8.256 ms | 121.1 | 123.5 |
| Candidate, 90-150 seconds | 8.435 ms | 118.6 | 120.8 |
| Candidate, 240-300 seconds | 8.348 ms | 119.8 | 122.1 |

The first minute uses reports ending 10-70 seconds after the ready/capture marker,
which itself follows 20 seconds of world settling. Captures match camera and
visible draw/shadow/bone/particle counts. Background source populations differ
on the live server. The baseline has a transient 73 FPS two-second interval,
so the average difference is not evidence of an equivalent steady-state gain.
The upper rates are best **two-second averages**, not instantaneous peak FPS.
The longer candidate run confirms the user's small early decline, followed by
partial recovery rather than a continuing five-minute collapse.

Over that stationary run, live UI resources stay at six meshes, three glyph
pages, 165 UI descriptor sets and one character atlas. Pending retirement batches
are zero at both endpoints. One additional M2 mesh and three common textures
are admitted; dynamic sources still change on the live server. Sampled process
private memory remains approximately 2.96-2.97 GB. These observations complement
the repeated-generation GPU regression; they do not establish an indefinite
memory bound for every content route. Raw logs, captures, timing windows and
memory samples are under `target/*foundation-still-*` and
`target/foundation-candidate-*` in the local validation workspace.

### Movement coverage and remaining cost

A twelve-second forward autorun segment starts at the same city position,
orientation and saved camera on both builds. It begins approximately two minutes
after the measurement marker (105 seconds baseline, 117 seconds candidate).
The normal keyboard binding starts and stops movement; no movement/visibility
implementation is replaced by the diagnostic harness. Server-saved coordinates
verify the travelled route: baseline `(1597.74, -4401.29, 7.62672)`, candidate
`(1598.61, -4401.18, 7.97266)`, both still at orientation `0.190609`. The 12.016
and 12.116 second input segments end within one yard horizontally. Soap is
restored to the starting city position afterward. Timing windows exclude the
start/stop edges.

| Route phase | Baseline | Candidate |
| --- | ---: | ---: |
| Before movement | 8.273 ms / 120.9 FPS | 8.490 ms / 117.8 FPS |
| During movement | 10.956 ms / 91.3 FPS | 11.213 ms / 89.2 FPS |
| Stationary at endpoint | 11.161 ms / 89.6 FPS | 11.119 ms / 89.9 FPS |

The movement drop remains. These runs do **not** demonstrate a material movement
speedup. Baseline/candidate live NPC populations and exact start times differ;
small differences must not be promoted into a gain or regression claim. The
important repeatable result is a roughly 3 ms increase that remains at the
endpoint after movement stops. Candidate endpoint M2 preparation is 5.501 ms,
including 4.652 ms in instance traversal, versus 5.472/4.629 ms on the baseline.
Candidate world service is 2.270 ms versus 2.377 ms baseline. The stage named
`local player residency` also includes other animation/movement work and is not
an isolated appearance-plan measurement.

The foundation contracts and their scaling/lifetime regressions are implemented,
but instance traversal remains a measured world-performance follow-up. Instancing
does not eliminate each model's simulation, pose and visibility preparation.
Regular implementation must not treat stationary FPS or this foundation delivery
as proof that moving scenes meet a frame-time target. Raw route timings and phase
breakdowns are retained under `target/foundation-route-*` and live logs under
`target/perf-four-hour-foundation-route-*`.

## Testing delivery

`scripts/build-client.ps1 -Profile test-client` reserved and compiled Solarity
0.0.3a **Build 131**. `scripts/install-test-client.ps1 -SkipBuild` installed it
through the existing 2560 x 1440 Testing launcher. Package and installed SHA-256
both equal `00C30145DA60729B513ED515DF311EBCC853721A23177287A07E5D83764DBB0E`.
The embedded source revision is `aec1632f5987016f3dcfbd6ec52208d11dd440b4`;
the dirty marker records the reserved `BUILD_NUMBER` and this delivery report,
not diagnostic Rust adapters. The numbered package compiles restored production
source. The installed Testing launcher starts the Build 131 window, admits its
authored Glue scene and closes through the normal event loop without an error.
This package record and all five foundation commits belong to
`perf/critical-frame-work`.
