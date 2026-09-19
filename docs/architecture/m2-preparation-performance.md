# M2 preparation: consumer demand, pose batches and shared registration

## Owned draw-phase cutover

The 2026-09-19 Build 155 live review found 3.424 ms/frame still in main-thread M2
admission. Its worker boundary excluded shadow-only palette/packet preparation
and left visible shadow packets on main. The current source moves complete
render palettes and both shadow collectors' material packets into the owned draw
phase. It keeps ordered gameplay callbacks on main, sampling only the bones they
consume. Root palettes already produced by the pose phase remain reusable.

`geometry/input.rs` distinguishes visible simulation from shadow-only demand.
Offscreen shadow work does not advance particle/ribbon clocks or transfer their
state. All packets initially use model-local bone offsets; ordered publication
appends palettes under the existing visible/actual-shadow-output rules and checks
relocation into final frame storage. Thus an alpha-rejected shadow-only model
does not shift later models' palette offsets. The extracted shadow packet kernel
retains the existing `834660` material/opacity selection and sampling order.

Small calibrated model jobs share groups of at most 16 models and a target of
100 microseconds of estimated work. Unknown/expensive jobs remain separate;
estimates never change simulation demand or promise preemption of an indivisible
kernel. Group records and shadow outputs use CPU storage reservations. Closing
admission submits the final partial group; abandonment returns unsubmitted and
submitted owned state without running the abandoned suffix.

The serial moving-scene comparison retains its original color/caster calculation
and shares only the extracted pure packet helper. It now includes explicit
offscreen casters, exact palette/draw equality, relocation overflow, unchanged
effect clocks/state, and frame abandonment/failure. Separate deterministic tests
cover grouping limits, uncalibrated work, refused dispatch and budget ownership.
This is an ownership/ordering proof, not a measured five-millisecond improvement.
Spatial admission, ordered callbacks, receiver queries and broad topology work
remain main-thread costs; see the complete CPU cutover checklist.

Formatting, full workspace Clippy with warnings denied, and all 1,588 workspace
tests pass (zero failures, 33 ignored). A hidden 168-frame debug replay with Soap's
appearance and 48 authored NPCs completed travel/orbit/settled phases with primary
and environment shadows and no warning/error logs. It checks integration only;
no live FPS gain is established. Evidence is recorded in the
[cutover checkpoint](cpu-cutover-status.md#owned-m2-draw-phase-checkpoint).

## Consumer-driven preparation

The traversal now resolves camera and light-volume admission after final
placement joins and before complete skeletal sampling. A model's scene callback
does not by itself request a render palette. Shadow-only models still sample a
complete palette and submit their admitted shadows independently of the camera.
The early worker batch accepts unattached roots admitted by either the camera or
the independent shadow collectors. Attached transforms are sampled after their
final admission in traversal. Shadow-only roots retain worker sampling instead
of being pushed back into the serial traversal by camera culling.

CPU queries use the separate `M2BoneSamples` type through `M2BoneTransforms`.
It has no palette accessor. Current event crossings, active item/effect/retired
attachments, mount-camera queries and authored light emitters request named
bones; the sampler closes their ancestry and uses the same clock, override,
billboard and matrix arithmetic as complete poses. Unrequested matrices cannot
be read from a prior model or frame. Callback-only models without active bone
consumers validate their inputs without touching palette storage. Ordinary unit
and mount callbacks use the same demand boundary, including expired variations.

This retains build-12340's independent `832450` callback traversal, `823F10`
render registration, and `831330` attachment queries. Event ordering, RNG,
completion, attachment enable tracks and offscreen light publication remain
ordered on the client thread. The new demand modules do not suppress those
callbacks or substitute the camera's admission for shadow admission.

Liquid/fog classification now follows camera admission. Color receiver requests
retain logical placement ancestry until mesh, particle and ribbon packets have
survived admission. Only their actual receivers and required ancestors perform
lighting callbacks, in traversal order, before scene-bank finalization. This
follows the registered-root lighting drain in `821A20` through `831AF0`.
An offscreen light emitter still contributes its light without allocating its
own unused receiver scene. Missing submitted receiver mappings are errors;
they cannot silently choose another light bank.

Focused folder modules own CPU demand, selected sampling, ordered publication,
receiver resolution and unit callbacks. Ordered publication accepts a borrowed
`M2BoneTransforms` capability and has no renderer or palette-upload interface.
All scratch storage is retained; no timing or per-model diagnostic logging was
added to the ordinary frame loop.

Regression coverage includes exact selected/full matrices with reversed parents
and changing overrides, shrinking/empty requests, failure invalidation, forward
receiver ancestry and frame reuse. Existing scene tests verify offscreen event
and effect construction with zero speculative unit palettes, offscreen animated
lights, independent shadow palettes, mounts, equipment and retirement. The same
offscreen callback fixture also admits primary shadows for fully opaque units
and checks that both palettes are consumed from the prepared batch.

The completed consumer-demand implementation passes `cargo fmt --all -- --check`,
workspace Clippy with all targets/features and warnings denied, and workspace
tests with all features: 1,390 passed, zero failed, 29 explicitly ignored.

The requested five-millisecond whole-frame reduction is not established by these
structural changes. The hidden diagnostic replay uses Soap's recorded appearance,
equipment, route and camera at 2560 x 1440 / Ultra, but an authored population of
120 Goblins rather than the captured live population. It advances animation on
a fixed 1/120-second clock and includes stationary, orbit and travel phases.
Asynchronous streaming still creates some differences in resident tile and draw
counts between runs. It cannot replace a matched live Soap measurement.

The replay's hidden Vulkan presentation blocks for several milliseconds, versus
roughly 0.1 ms in the earlier live capture. Therefore changes in its total frame
time or FPS cannot be credited to M2 preparation. Component comparisons must
use the M2 profile scope and omit intervals spanning phase boundaries. The first
candidate also excluded offscreen shadow casters from worker sampling, pushing
their full palettes into serial traversal; the final batch admission explicitly
includes both camera and independent shadow collectors.

The corrected baseline/candidate/candidate/baseline replay on 2026-09-14 measured
the following weighted M2-scope means, using only intervals wholly inside each
phase. Both executables used the same temporary diagnostic adapter and four CPU
workers. Each run contained 512 frames per phase; boundary-spanning profiler
intervals are excluded from this table.

| Phase | Build 132 source (ms) | Consumer demand (ms) | Difference (ms) |
| --- | ---: | ---: | ---: |
| Stationary | 2.739 | 2.502 | 0.238 |
| Orbit | 2.529 | 2.032 | 0.497 |
| Pointer | 3.037 | 2.864 | 0.172 |
| Travel out | 2.986 | 2.683 | 0.303 |
| Travel back | 3.032 | 2.768 | 0.264 |
| Settled | 3.289 | 3.141 | 0.148 |

These component results are substantially below the requested 5 ms reduction.
The frame-time means were generally worse in the candidate hidden runs, while
queue presentation waited longer; neither a live frame-rate gain nor a live
regression can be inferred from that comparison. Terrain, WDL, WMO and primary
shadow draw counts matched at corresponding frames in the first pair, but M2,
far environment-shadow and particle counts had some differences during streaming.
This work establishes the CPU demand boundary; it does not complete the
longstanding M2 frame-cost fix or establish doubled/tripled FPS.

The production source passed the required checks before the diagnostic build;
the adapter restored those exact source bytes afterward. The installed Testing
client remains Build 132. No new client package is represented by these results.

The measurements below describe the earlier Build 132 work. They do not establish
a whole-frame saving for consumer-driven preparation.

## Build 132 measurements and implementation

Build 131's Orgrimmar route ended with approximately 5.5 ms of M2 preparation,
including 4.65 ms inside placement traversal. Instanced submission did not remove
this CPU work. Sparse diagnostic probes attributed about 1.5 ms to bone poses,
1 ms to unit/object liquid, fog and receiver lighting, and 0.8 ms to particles.
Those probes were temporary and are not part of the shipped frame loop.

The first registration/empty-track candidate alone was insufficient. In a single
diagnostic process alternating original and candidate paths every 15 seconds,
the 10-90 second window measured 6.187 / 5.586 ms mean M2 preparation. Mean unit
and object counts were 115.9 / 116.6, with other live scene variation. This is
approximately 0.6 ms, not evidence of doubled or tripled frame rates. A later
workload change in that run must not be averaged into a performance gain.

## Frame-local unit pose batches

The ordered unit callbacks already select the frame's animation clocks before
ordinary draw preparation. Previously, the traversal then sampled each unit's
entire skeleton serially, interleaving pure arithmetic with spatial queries,
attachments, shadows and GPU packet creation. The GPU instancing boundary did
not parallelize that work.

`m2/preparation/poses` now snapshots those selected inputs into owned CPU jobs.
Each job owns only a shared decoded model, current clocks and overrides, camera
transform and reusable pose storage. It contains no renderer handles, gameplay
`Rc` state, random stream or callback. The existing CPU executor processes the
borrowed slice through one bounded permit and joins every item before returning.
No per-model task channel or new pool is created.

The serial traversal consumes a palette only when its model generation, primary
clock, complete model-view matrix, finger clock/mask, billboard exceptions,
semantic transforms and bone-sequence clocks all match. Consumption swaps the
palette storage instead of copying it. A changed attachment or late owner input
invalidates that prepared result and uses the current ordinary computation.
Unconsumed errors are not published for a model that never reaches its pose
consumer. Accepted errors retain the original traversal point.

Small batches compute on the calling thread. Busy streaming also retains the
calling-thread execution when fewer than two worker lanes are available; frame
work must not wait behind an asset queue merely to gain parallelism. Removed
owners release their model-generation references and palettes on the next batch.
There is no cross-frame pose reuse or visibility approximation. The existing
opt-in profiler now reports `unit pose batch` separately from traversal; disabled
profiling adds no timer calls or per-model counters.

## Exact shared unit registration

Admission, model scene callbacks, floor lighting, area selection and ground sound
called the same Unit_C point-registration algorithm independently. Per-owner
lighting retention helped stationary models, but did not share the result across
consumers of a moving unit. Repeated queries traversed terrain and WMO/BSP data
again even when the position and geometry were identical.

The resident map now shares successful `WorldModelRegistrationSelection` values
by the exact three coordinate bit patterns and their movement dependencies.
Terrain publication and WMO root replacement or removal invalidate the cache.
Root motion invalidates only points whose vertical probes intersect the union
of its previous and current XY bounds, using the existing lighting dependency
tracker. Disjoint ship motion no longer clears every unit registration. Active
entries advance their revision token; entries older than the bounded motion
history repeat the native query. A new map owns a new cache. Errors are not
cached. The original downward probe, upward retry, root ordering, portal/floor banks and terrain occlusion run
unchanged on a miss. GameObject box registration remains a distinct algorithm.

The cache holds at most 2,048 positions. Old walking positions are evicted in
insertion order; eviction repeats the existing query. It does not round positions
or retain historical geometry. Capacity remains bounded during continued motion.
Registration is decomposed into a folder facade with query, receiver, result and
cache modules.

## Immutable bone work

Decoded animation data now records which individual bones have no keys in any
translation, rotation or scale channel. Their local transform is identity unless
an instance supplies a semantic bone override. Parent inheritance and billboard
composition still run. This extends the existing whole-skeleton identity case to
untracked bones within animated skeletons.

Pose composition also consumes the parent order already validated at asset
admission. It no longer resets a visit-state array, recursively rediscovers that
order, or clears a palette whose entries will all be assigned. Track sampling,
sequence blending, finger overlays, override order and matrix arithmetic remain
unchanged for animated bones. No visibility, event, attachment or particle update
is skipped by these changes.

## Verification

Regression coverage compares shared registration with the original query against
an authored interior floor, verifies coordinate changes and root removal, checks
invalid input and bounded walking history, and exercises reversed bone order with
animated parents and changing overrides. Existing native floor, pose, billboard,
attachment, animation and rendered-frame oracles remain part of the required
workspace checks.

Additional tests compare worker palettes exactly with serial sampling across
moving cameras, billboard masks, sequence overrides and semantic transforms.
Every prepared-input dependency and one-use publication is checked. The existing
offscreen water-effect regression checks that both callback-selected unit poses
are consumed without resampling while event order and effect visibility remain
unchanged. CPU executor tests cover borrowed inputs, joined private workers,
admission release and recovery after a task panic.

`benchmark_moving_unit_pose_batch` is an ignored CPU-only test using locally owned
Orc, Tauren, Troll and Human male models. It samples 120 owners with independent
standing/walking clocks and a changing camera. Every measured frame runs both
paths in alternating order, excludes 20 warmup frames, and compares every final
palette exactly. It measures skeletal sampling, not complete M2 preparation or
2K/Ultra frame rate. It opens no window and uses no server or player input.

An optimized `test-client` run on the local i5-9600K with four CPU workers measured
1.2380 ms serial and 0.8130 ms batched across 280 paired frames: 0.4250 ms less
skeletal sampling, or 1.52 times the component throughput. Exact palettes matched.
This was measured after immutable bone-work changes on both paths; it does not
compare those changes with Build 131. Collection, exact-key checks, and other M2
work are outside this benchmark. The subsequent source changes reorganized the
module, tightened executor admission and registration invalidation, and added
model-generation rejection coverage; they did not change sampling arithmetic.

These figures cannot be added to the earlier live diagnostic as a claimed frame
saving: the workloads and measured boundaries differ. All required workspace checks pass: formatting, Clippy with warnings denied,
and 1,389 passing tests with 29 explicitly ignored tests. The optimized benchmark
was run separately. The remaining liquid/fog, receiver, particle and other frame costs
are not eliminated by the pose batch; total FPS improvement needs a matched live
test after installation.

## Build 132 delivery

- Product: `0.0.3a`, Build `000132`, profile `test-client`.
- Source: `e5465b839d49eee3c9eaf5d8124c231526417c09`.
- Build-time dirty state consists of the packaging script's reserved
  `BUILD_NUMBER`; the implementation was committed before packaging.
- Package and installed Testing executable SHA-256:
  `5CD5CCC017E41CD6601829D7DFC0AE69C0CBBD03F98B85B5AE1E8765700F1CED`.
- Installed at `2026-09-13T22:14:04-04:00` using `-SkipBuild`, with the existing
  2560 x 1440 Testing launch configuration and normal profiling disabled.
- Formatting, Clippy with warnings denied and the full workspace tests passed
  before packaging (1,389 passed, 29 explicitly ignored). The optimized package
  compiled successfully and reported the expected version and build identity.

The client was not launched for a live scene measurement during delivery because
this work was kept off the user's active desktop. Build 132 therefore has no
matched stationary/moving whole-frame FPS result yet. The earlier downward FPS
trend and the requested two- or threefold frame-rate increase remain unresolved
measurement targets, not claims of this delivery.
