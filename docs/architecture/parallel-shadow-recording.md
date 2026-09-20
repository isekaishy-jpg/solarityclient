# Parallel shadow command recording

This boundary moves Vulkan shadow command encoding onto the existing CPU executor.
It does not move simulation, camera sampling, gameplay publication or queue submission.
The controlled comparison below measures this boundary; it does not complete the CPU cutover.

## Ownership and ordering

`vulkan_world_frame/recording` captures up to four owned pass inputs: primary shadows,
then the three active environment updates. Compact commands resolve the same immutable
resource joins as the previous direct recorder. Pass command storage uses retained
`CpuBuffer` allocations charged to CPU Frame/Result storage. Four owners retain their
own capacities; there is no accumulating list of historical frame generations.

Each world frame slot owns four independent Vulkan command pools and primary command
buffers. Pools are created lazily for shadow rendering, reset only after the slot's
existing GPU fence, and destroyed under the renderer's teardown barrier. Workers never
record through the main scene pool or one another's pools. This is required by Vulkan's
[command-pool external synchronization contract](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html).

The main thread records the independent scene while workers encode shadows. One scoped
owner pins renderer resources and pools until every job has returned, including domain
failure, native servicing failure and unwind. The main thread then submits the original
primary/environment order followed by the scene, in one queue submission using the
existing slot fence and presentation semaphore. It adds no frame of latency or GPU
frames in flight. Vulkan submission order alone is not a memory dependency; the original
attachment/image barriers remain between passes, including shared environment depth,
initial cache clears and write-to-sample transitions. See the
[synchronization specification](https://docs.vulkan.org/spec/latest/chapters/synchronization.html).

`WorldFrameExecution` makes the existing executor and consumption policy explicit.
Production uses `FrameWait` to service queued native input if recording is still pending.
Offline callers explicitly wait on their supplied executor. There is no renderer-owned
thread pool or old serial recording fallback. The next ordinary camera/update boundary
still consumes queued input; workers never mutate gameplay state.

## Behavioral authority

This preserves the stock-evidenced shadow contract already implemented: `7BBC50` scenery
WMO before scenery M2 before primary unit silhouettes, and `874890` cached environment
map publication. Material indices, adjacent scenery instances, cascade masks, viewport
rectangles, clears, palette offsets and source draw order remain unchanged. The older
3.3.5 evidence remains the authority; modern Classic contributes no new behavior here.

GPU timestamps begin in the first submitted shadow buffer and continue at the existing
scene boundaries. F10 adds sampled shadow capture, worker recording CPU/wall cost and
pending-consumption scopes; disabled instrumentation creates no per-draw logs.

## Validation and limits

Workspace Clippy and formatting checks pass. The full workspace run passed 1,623
tests with 33 existing ignored; the subsequently added recording-lifetime test
passed with all 49 rendering unit tests, for 1,624 distinct passing tests across
those checks. Optimized comparisons are recorded below.
Tests extend the existing stock receiver/caster fixtures across repeated pool reuse and
quality changes, compare full cache images with one and four workers, and force native
wait error/unwind while jobs still own resources. The last fixture verifies complete
reclamation in original pass slots before the scope exits.

This addresses independent command encoding. Main-thread M2 admission, receiver queries,
packet assembly, ordinary scene command recording, other resource domains and the full
CPU/cache cutover requirements remain open. Main capture is deliberately measured as
part of the recording interval; moving driver calls does not make that transfer free.

## Controlled 192-NPC comparison

The baseline is Build 165's validated palette-page benchmark executable
(`7332569DBEDB22E319BEC1571CB7DA45B72BC8498A946A3402A1D9414D5AFF7F`).
The parallel-recording candidate is
`CDC77269CC13A7E62D4219AB9ADCD1A3527671321A58673EBE30E161A32BDFF6`.
Both use the optimized `test-client` profile, four shared CPU workers, Soap,
192 authored NPCs, 2560x1440, Ultra shadows, and identical offline scene/camera
inputs. No compiler or other diagnostic GPU test ran during measurements.
Hidden surfaces do not establish desktop or live-server FPS. The fixture has no
network, movement solver, audio or overlays.

Four alternating unprofiled runs each contain 1,024 streaming, stationary,
orbit and pointer-motion frames: 16,384 frames total. Matched stationary rows
have two resident tiles, no newly admitted tile, 243 WMO draws, 482 far-shadow
casters, 1,658 M2 draws and 23,864 palette bones. Matching is necessary because
asynchronous residency progresses at different frame ordinals.

| Metric (ms) | Baseline 1 | Candidate 1 | Baseline 2 | Candidate 2 |
| --- | ---: | ---: | ---: | ---: |
| Matched stationary median | 6.5232 | 5.7856 | 6.5184 | 5.9008 |
| Matched stationary p95 | 7.4615 | 6.9828 | 7.4389 | 6.8878 |
| Orbit median | 5.8436 | 4.9310 | 5.7754 | 4.9791 |
| Pointer-motion median | 7.3400 | 6.6775 | 7.4575 | 6.6713 |

Matched stationary sample counts are 355, 368, 395 and 405. Median reductions
are 0.7377 and 0.6176 ms. This is below the requested 5 ms overall reduction;
it is not a twofold or threefold FPS gain. Isolated long frames remain: stationary
maxima across all rows were 40.13, 32.13, 10.59 and 32.66 ms. The first candidate's
matched subset includes a 32.13 ms frame; the second candidate's matched maximum
is 8.39 ms. No tail-latency fix is claimed from these medians.

A separate profiled baseline/candidate pair completed 8,192 frames. Each has
4,064 ordinary CPU frames and 32 detailed GPU samples. Mean ordinary main-thread
recording interval falls from 1.5866 to 1.0206 ms; complete Vulkan CPU scope falls
from 2.5304 to 1.9528 ms. Candidate capture costs 0.2046 ms/frame. Across all four
workers, recording sums to 0.7555 ms/frame; these overlapping intervals must not
be added to the main interval. Main waits for recording on 269 ordinary frames,
averaging 0.0064 ms over all ordinary frames (0.0960 ms per wait, maximum 0.4884).

GPU total mean is 2.9669 ms baseline and 3.0511 ms candidate; shadow interval mean
is 1.3360 versus 1.4149 ms. This sparse sample shows a small GPU cost increase,
not a GPU improvement. The combined unprofiled frame-time result remains lower.
GPU intervals include pipeline overlap and are not isolated shader costs.

The profiled candidate's largest non-startup frame is 58.98 ms, including
42.95 ms in existing static M2 admission capture. Another frame spends 22.51 ms
in the recording interval, while candidate shadow pending never exceeds 0.49 ms
and worker recording never exceeds 2.58 ms. Baseline also records a 26.10 ms
command interval and a separate 27.81 ms geometry-consumption interval. These
boundaries identify remaining stalls; wall timings alone do not distinguish
preemption from useful CPU work. They do not justify blaming the new pool wait
or claiming those stalls are fixed.

Ignored artifacts: `target/shadow-recording-comparison-*`,
`target/shadow-recording-profile-*`, `target/shadow-recording-profile-analysis.json`,
`target/shadow-tail-review.py`, `target/shadow-{test,clippy}.*`, and
`target/shadow-final-*`. The full CPU/cache requirement list remains authoritative
in [cutover status](cpu-cutover-status.md#still-required-for-the-complete-cutover).
