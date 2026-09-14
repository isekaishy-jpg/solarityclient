# M2 preparation: unit pose batches and shared registration

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
