# Effect storage and native GPU dependency consumption

This connects retained effect and named-bone storage to CPU admission, and extends
the renderer's existing completion thread to acquisition and retirement operations.
It does not claim that the remaining architecture in the
[cutover status](cpu-cutover-status.md#still-required-for-the-complete-cutover)
is complete.

## Storage ownership

Particle simulation owns a unique reservation covering actual particle, active-slot
and free-slot capacity. Admission reserves the authored maximum before worker
transfer without changing the stock logical pool limit, emission remainder, RNG
state or active/free identities. Existing address-dependent twinkle relocation
semantics remain in force. Old and replacement allocations are both charged during
growth; refusing admission preserves simulation values. Bound workers cannot grow
past that admitted capacity.

Ribbon history owns its retained ring charge. Insertion now removes the oldest
section before inserting into a full ring, preserving the authored final history
without temporary growth followed by truncation. Named CPU bone sampling admits
its ancestor marks and nested pose scratch before ordered callbacks, including
offscreen callback consumers. Charges remain with their allocations across job
transfer, result consumption and reset; destruction releases them.

These are adopted allocation charges, not a measurement or limit on process RSS.
Never-dispatched effect owners and ordinary asset/cache buffers are not universally
covered. Total working-set admission and cache lifecycle remain open requirements.

## GPU dependencies

The existing renderer-owned completion thread now handles three typed operations:
frame fence observation, image acquisition, and device-idle retirement. General
CPU workers never execute these blocking driver calls. One reusable request/result
cell retains the originating trace, handles and operation-specific error mapping.
Callback failure or unwind drains the active host operation before ownership can
return to the renderer.

Acquisition probes once with timeout zero. A ready image is consumed immediately;
only `NOT_READY` transfers the same swapchain and selected slot semaphore to the
completion thread. Other errors retain their normal terminal or out-of-date result.
The Vulkan [window-system integration specification](https://docs.vulkan.org/spec/latest/chapters/VK_KHR_surface/wsi.html)
defines unsuccessful acquisition as leaving its synchronization objects unaffected.
There is no polling loop, second successful acquisition, or additional frame in flight.

Native world/Glue, UI/loading and cinematic presentation service queued window
events while these acquisitions are pending. Existing world extent/resource and
shadow-quality retirement barriers use the same service. Swapchain recreation
retains the caller's execution context while retiring device users before destroying
old handles. Presentation marks possible device use before invoking the recording/
submission path because an out-of-date error may follow a successful submission.

Native callbacks have readiness access only. They cannot submit GPU work, mutate
the renderer, dispatch gameplay events, or resample the current camera. Explicit
offline presentation retains synchronous consumption. Standalone diagnostic model/
terrain presentation and exceptional teardown still retain their existing waits.
Required asset/loading consumption and remaining upload boundaries still need the
broader integration documented in the cutover status.

Pose composition and renderer presentation are now folder-backed modules. Pose
sampling, storage ownership, cinematic presentation, UI presentation and the common
swapchain lifetime wrapper live in separate children.

## Validation

Controlled effect tests compare admitted and unbound simulation across 128 updates,
including variable deltas, changed transforms, admission refusal, reset and release.
Ribbon history remains within its original allocation under alternating short and
long steps. Named-bone tests compare sparse samples with full stock poses and check
charge lifetime.

Channel-controlled completion tests cover ready-image bypass, delayed acquisition,
exact handle/index preservation, out-of-date propagation, failed native callbacks,
device-idle draining and service reuse. Existing panic, notifier, multi-reader and
trace ownership fixtures exercise the common service. Real GPU UI texture/mesh
lifetime and cinematic pixel-capture tests now use the serviced presentation APIs.

Formatting, workspace Clippy (all targets/features, warnings denied), and the full
workspace test suite passed: 1,659 tests, zero failures, 33 existing ignored tests
across 103 suites. Logs are ignored `target/cpu-effects-{clippy,test}.*.log`.
No performance improvement is inferred from compilation or from moving a driver
wait off the coordinator.

## Crowded movement and worker-count run

The optimized candidate SHA-256 is
`55BDDA5828C7873CF1D9684022CA394460A55AD77914993C6ACE8029CCFCAAB0`.
Four sequential hidden-window runs used Soap and 300 equipped NPC fixtures at
`(1515.34, -4417.27, 18.0499)`, 2560 x 1440, VSync off and shadow quality 5.
Each completed 896 frames: 128 each of streaming, stationary, orbit, pointer
camera, travel out, travel back and settled reentry. The travel offset was
`(-80, 15, 0)`. All worker counts completed without a crash or storage refusal.
Compilation and GPU tests did not overlap these runs.

| Phase | 1 worker median ms | 2 workers | 4 workers | 8 workers |
| --- | ---: | ---: | ---: | ---: |
| Streaming | 32.849 | 31.737 | 25.790 | 26.005 |
| Stationary | 31.788 | 31.374 | 30.732 | 28.225 |
| Orbit | 22.258 | 18.966 | 21.207 | 17.856 |
| Pointer camera | 32.589 | 32.196 | 29.456 | 33.486 |
| Travel out | 32.124 | 26.745 | 29.197 | 25.749 |
| Travel back | 32.102 | 24.086 | 24.023 | 25.427 |
| Settled | 32.683 | 30.218 | 31.613 | 25.749 |

This is mixed scaling, not proof of the target throughput or an optimal worker
count. The one-worker first frame took 909.676 ms, including 891.442 ms inside
presentation. The four-worker first frame took 102.101 ms. Four-worker travel-out
p95 was 81.652 ms and eight-worker pointer p95 was 64.818 ms. These tails remain
visible limitations; the run does not isolate their causes.

The initial four-worker Build 172 control was unusually slow (52.045 ms stationary
median), compared with its prior recorded runs. It cannot establish a large gain
from this change. A second four-worker pair checked this variance:

| Phase | Build 172 median ms | Candidate median ms |
| --- | ---: | ---: |
| Streaming | 40.191 | 32.281 |
| Stationary | 37.654 | 32.805 |
| Orbit | 19.709 | 20.848 |
| Pointer camera | 26.481 | 32.717 |
| Travel out | 22.985 | 28.782 |
| Travel back | 22.764 | 30.571 |
| Settled | 33.121 | 32.784 |

The repeat also completed without errors. It does not establish a consistent gain:
several moving phases are slower. Tile activation timing differs during travel
(the control retains one tile through travel-out while the candidate reaches two),
so those phases do not contain identical loaded scenes. No causal attribution or
large performance improvement is established. All seven diagnostic runs together
completed 6,272 frames. These fixtures
exclude live network, the movement solver, audio and overlays; they do not measure
desktop FPS or verify visible camera smoothness. Raw artifacts are ignored
`target/cpu-effects-worker-sweep-*`, `target/cpu-effects-baseline-control-*`,
`target/cpu-effects-baseline-repeat-*` and `target/cpu-effects-candidate-repeat-*`.
