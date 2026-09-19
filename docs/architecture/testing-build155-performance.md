# Build 155 live F10 review

## Capture and limits

The user's 2026-09-19 capture is `capture-1789843563290-1`, from Testing
Build 155, source `4dd6a28ddda06034aaa8be34b688f0cf06150167`. The capture
lasted 65.813 seconds. Its manifest reports four CPU workers, two network
workers, zero dropped samples/events/trace rows and zero capacity overflows.
The matching `solarity-20260919-144530.log` records 2560 x 1440 on a
GeForce GTX 1070 and a pool with three protected workers and one flexible worker.

Analysis excludes writer intervals ending in the first two seconds. That leaves
7,081 ordinary frames and 56 detail frames. Ordinary frames still include F10
coarse instrumentation. Detail frames are sampled every 128 frames and have
additional observer cost. Timings below are inclusive and overlapping unless
explicitly described otherwise; main, worker and GPU durations must not be added.

This is a moving live run, not a controlled before/after comparison. It establishes
current costs, not a regression caused by Build 155 or an improvement over stock.
The capture began after world entry, so it does not measure initial loading.

## Whole frames and CPU distribution

| Observation | Result |
| --- | ---: |
| Ordinary mean frame | 9.059 ms, approximately 110.4 FPS |
| Ordinary p95 / p99 | 12.571 / 17.436 ms |
| Largest ordinary frame | 29.875 ms |
| Detail mean frame | 11.154 ms |
| Main thread CPU utilization | 98.44% of one logical CPU |
| Protected workers, individually | 10.73%, 10.93%, 11.14% of one logical CPU |
| Flexible worker | 13.10% of one logical CPU |

CPU utilization comes from OS user/kernel-time deltas between resource snapshots
at 1.006 and 65.780 seconds. These percentages are per logical CPU, not the whole
six-logical-CPU machine. An additional unlabelled OS thread, ID 28428, used 14.09%
of one logical CPU; the capture does not identify its owner. Do not attribute it
to audio or the graphics driver without further evidence.

The pool executes real work, but the main thread remains saturated while workers
have considerable unused capacity. The connected worker kernels do not account
for enough of the current critical path. Whole-stage admission/publication work
remains necessary; changing worker count alone is not supported as the remedy.

## Main-thread costs and confirmed code paths

| Inclusive ordinary-frame scope | Mean per frame |
| --- | ---: |
| World preparation: M2 admission | 3.424 ms |
| World/session service | 1.728 ms |
| Vulkan presentation | 1.739 ms |
| Vulkan command recording, inside presentation | 1.032 ms |
| M2 and lit-surface publication | 0.666 ms |
| World FrameXML update/upload | 0.669 ms |
| Local-player synchronization | 0.036 ms |

The continuing M2 serial path is concrete:
[`preparation/frame/admission.rs`](../../crates/runtime/src/application/terrain_frame/m2/preparation/frame/admission.rs)
performs placement selection, animation/attachment joins, shadow admission,
remaining pose sampling, ordered CPU output, shadow packet creation and
liquid/fog queries before queueing each visible model's geometry job.
[`geometry/publication.rs`](../../crates/runtime/src/application/terrain_frame/m2/preparation/geometry/publication.rs)
also prepares/reserves each job's owned inputs on main. Moving geometry execution
and some root poses did not move this whole stage. Sampled scenes visit about
2,926 placements per frame, with 2,244 early rejections; early rejection itself
is not evidence that stock-required shadow/callback work can be omitted.

Ordinary `cpu.frame.execute` spans average approximately 7-8 microseconds per
kernel across the workers. Queueing, ownership transfer and observer costs are
outside parts of that span. This makes batching/coarser independent preparation
a relevant investigation, but the capture does not isolate scheduler overhead
well enough to assign a millisecond saving to that change.

The new local-player synchronization is not a dominant steady-state cost in this
run. No CPU failure, lost-result or capacity-failure message appears in the log.
Existing optional Tauren texture warnings are present; they do not demonstrate
a new concurrency failure. Neither observation proves every lifecycle correct.

## Movement and publication spikes

On 407 ordinary frames, `m2.topology.rebuilt_placements` reports an average of
25,580 entries. There are only 22 ordinary static spatial-index rebuilds, so these
must not be described as 407 full spatial-tree reconstructions.

The distinction matters: immutable static facts are reused, but
[`visibility/publication.rs`](../../crates/runtime/src/application/terrain_frame/m2/visibility/publication.rs)
still clears and reconstructs broad parallel arrays by walking the placement
lineage. [`topology/publication.rs`](../../crates/runtime/src/application/terrain_frame/m2/topology/publication.rs)
calls that operation for ordinary topology changes. This preserves metadata
calculation without eliminating whole-population publication work.

Topology-publication spans reach **4.930 ms**. Frames with topology publication
average 14.880 ms versus 8.704 ms for other ordinary frames, but those cohorts
have different scene/movement work: their difference is not the isolated cost of
topology. Terrain service reaches **13.414 ms**. For example, ordinary frame 4199
takes 27.284 ms, including 13.414 ms terrain service and 6.743 ms M2 admission.
Movement publication remains a multi-millisecond cutover requirement.

Ordinary frame 5046 takes **29.875 ms**, including 14.182 ms in
`ui.c_simple_render.prepare_with_glyphs`; its `textures` phase is **13.045 ms**.
[`c_simple_render.rs`](../../crates/ui/src/render/c_simple_render.rs) shows that
this phase builds texture quads and scroll/clipping inputs; it is not a texture
file read or GPU upload measurement. The targeted publisher in
[`c_glue_mgr/publication.rs`](../../crates/ui/src/glue/c_glue_mgr/publication.rs)
can still call that complete render-plan builder. The scope localizes the spike,
but it has no child attribution separating iteration/allocation from OS
descheduling. It does not prove a 13 ms glyph rasterization problem.

## Measurement defects and observer effects

`cpu.frame.result_wait` is currently misnamed. In
[`cpu/pool/batch/results.rs`](../../crates/cpu/src/pool/batch/results.rs), the scope
is entered even for ready results and nonblocking consumption, and extends over
the consumer and result-lease return. Its roughly 0.227 ms/frame is therefore
not evidence of 0.227 ms blocked time. Actual waiting and consumption need
separate attribution; this review does not change production instrumentation.

Sampled phase drain tails average **0.0272 ms** each, with a **0.1327 ms** maximum.
These measure final kernel start to final kernel return, not complete phase time
or queue residence. Together with CPU utilization they argue against treating
long end-of-phase worker waits as the main explanation of this run.

Detail frames are a median **2.136 ms** slower than the median of nearby ordinary
frames (up to four on either side). Motion and scene differences remain possible;
this is evidence of meaningful observer perturbation, not a calibrated overhead
subtraction. Use ordinary frames for the headline and detail frames for causal
structure. Disabled-versus-enabled whole-frame overhead remains unmeasured here.

## GPU and memory

The 56 post-warmup GPU samples average **3.401 ms**, including **1.502 ms** shadows.
Current CPU work is the larger bottleneck, but 1200 FPS requires approximately
0.833 ms/frame. CPU parallelism alone cannot meet that target with this GPU
workload, resolution and device. This does not establish a proposed GPU fix or
authorize changes to stock shadow visibility.

Working set starts at 2,020.7 MiB, reaches about 2,145 MiB and ends at 2,092.1 MiB.
Private bytes start at 3,077.6 MiB, peak near 3,247.7 MiB and finish at 3,182.5 MiB.
Memory remains large, but rises, plateaus and then falls; this run does not show
continuous accumulation or establish a leak. Full ownership/byte accounting and
retirement remain required by the resource design.

## Consequence for the remaining cutover

Prioritize the remaining serial M2 admission and broad residency publication,
then the UI render-plan rebuild path. These are observed frame-path costs. Keep
ordered gameplay callbacks/RNG separate from independently schedulable work and
preserve shadow/attachment consumers. Correct wait/consumption attribution before
using those counters to justify another scheduling change.

Shared nested-asset loading, complete memory/cache budgets and retirement,
remaining loading/upload/acquire/growth continuations, bulk-step sizing and
matched motion/loading/overhead verification are still mandatory. The complete
list remains in [CPU cutover status](cpu-cutover-status.md#still-required-for-the-complete-cutover).

## Evidence and reproduction

Raw files are in `%LOCALAPPDATA%/SolarityClient/testing/Profiles/`; the trace SHA-256
is `C0DDE0246175DC680DF7443E4CDB513D58805397B127A25996B9283DCE0B097D`.

Use `scripts/analyze-profile.py` on the primary CSV and
`scripts/analyze-trace.py` on its `.trace.csv` sibling. Sampled frame 3201 is the
median sampled frame by duration; frame 4993 is the slowest sampled frame.
The largest ordinary frame, 5046, has coarse/slow-event evidence rather than a
complete detail trace. Ignored `target/build155-live-audit.py` reproduces the
additional aggregates, resource deltas and frame cohorts. Reports are retained
as `target/build155-live-profile.txt`, `target/build155-live-audit.txt` and
`target/build155-live-{median,slowest}.json`.

Only analysis/documentation changed during this review. Build 155 remains the
installed executable; no new performance improvement is claimed.
