# Owned world command recording

World terrain, WMO, ground-detail, liquid, M2, particle and ribbon commands now
record on the shared CPU executor. Main captures numeric Vulkan commands in the
existing stock traversal, including compatible M2 instances and the fog changes
consumed by later ribbon draws. Workers receive contiguous ranges of that stream;
completion order does not change GPU execution order.

Each range owns a separate secondary command buffer and command pool. It
establishes its own viewport, scissor, pipelines, descriptors, buffers and push
constants. The enclosing dynamic-rendering scope executes the small sky/WDL
prefix, worker ranges, then the underwater/glare tail. Attachment formats and
sample count are explicit inheritance inputs. The parent frame-slot fence
protects pool reset and destruction, including all secondary buffers.

Postprocessing and UI use a second main-owned primary buffer. Main records it
while world recording jobs execute, then submits shadows, world and compositor
in that order. Existing image barriers, fog order, blend order and query order
remain intact. Camera sampling and gameplay callbacks are unchanged.

The implementation follows the Vulkan requirements for [command-pool external
synchronization and independent command-buffer state](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html)
and [dynamic-rendering secondary inheritance](https://docs.vulkan.org/spec/latest/chapters/renderpass.html).
This is an execution-policy change, not a different stock draw selection rule.

Compact command buffers reserve their complete raw-draw bound against CPU Frame
storage before dispatch. Instancing reduces that bound. At most 32 ranges are
active, with at most twice the configured worker count; small scenes create fewer
ranges. The current policy targets at least 128 commands per range before the
worker-count bound. Measured range costs feed the existing cost-aware scheduler.
Unused retained ranges retire after scene contraction. Only pools reachable by
the configured worker count are created.

The pending owner borrows the original resource context and slot pools, joins on
normal consumption, and unconditionally reclaims on failure or unwind. Required
joins service native input through the existing completion interface. Graphics
queue submission remains main-owned. No renderer registry lock, gameplay object
or borrowed resource collection crosses the worker boundary.

F10 exposes `rendering.scene.capture`, `rendering.scene.record`,
`rendering.scene.record_cpu`, `rendering.scene.commands`,
`rendering.scene.compositor` and `rendering.scene.pending`. Existing GPU phase
timestamps retain their submission positions. Disabled tracing does not add
per-command clocks.

The command implementation is decomposed into context, capture/draw lookup,
stock order, sky/compositor layers, frame orchestration and queue submission.
Worker command ownership, recording and batch lifecycle have their own folder.

Generated stock WMO green and M2 white/failure textures now use the same deferred
transfer lifetime as authored textures. Their exact pixels, color space, registry
identity and queue order are unchanged. Metadata/view setup completes before
submission, and the registry retains staging until its fence signals. This removes
the remaining synchronous texture-transfer uploader without adding a readiness
fallback or changing when subsequent graphics-queue commands may sample it.

## Validation

Workspace Clippy and formatting pass. The complete workspace suite passed
1,668 tests with 33 existing tests ignored; the focused stock renderer suite
passed all 195 tests. The first full run exposed excessive inline batch metadata
on the renderer's stack; moving that metadata to budgeted heap storage corrected
it before the passing runs. Controlled tests cover command/query ordering across
ranges, contraction without stale commands, and native-wait failure/unwind
reclamation.

The optimized 896-frame, 300-equipped-NPC replay completed successfully across
seven phases. A separate 112-frame capture run and the preserved pre-renderer
binary completed at the same settings. Reviewed stationary and travel captures
preserve scene geometry and compositor output. Animation time is wall-clock
based, so this is visual comparison rather than pixel equality. The travel path
goes beneath terrain in both versions and does not certify live movement.
Neither replay produced stderr diagnostics. The candidate executable is
`target/cpu-scene-candidate.exe`, SHA256
`A292E296854CDE8F571B7C4BAE38DBE7FC6ABC52B8A05546A50C1F6B1A571C66`.
No Vulkan validation layer was available, and no live FPS gain is claimed.
The subsequent combined source, including shared-WMO continuations and deferred generated textures, passed all 1,676 workspace tests with zero failures and 33 ignored, plus formatting and workspace Clippy. Final optimized replay qualification and the numbered package are pending.

The preceding loading-suspension source passed the complete workspace suite and
an optimized hidden 896-frame replay with 300 equipped NPCs, four workers and
seven stationary/orbit/movement/loading phases. That fixture has no network,
movement solver, audio or overlay; its hidden presentation timing is not desktop
FPS evidence. Its executable remains preserved separately as
`target/cpu-suspend-candidate.exe`.

## Combined-source qualification

The final combined source passes 1,676 workspace tests, zero failures and 33
ignored, plus workspace Clippy with warnings denied. Only leading documentation
comments changed after that run; final formatting and whitespace checks pass.
The optimized candidate is `target/cpu-group-candidate.exe`, SHA256
`5114EFF02629553EC242DFC7E2109D61437C17D957E02BDE165A9C3BE206E0B1`.

Hidden 300-equipped-NPC replays completed on 1, 2, 4 and 8 workers: 448 frames
each, 1,792 total, without crashes or stderr errors. These qualify progress and
lifetime under different execution plans. They do not establish a scaling gain:
loading schedules differ and the formatting check overlapped part of that series.

A subsequent sequential F10 pair used the preserved suspension-only binary and
the combined candidate, four workers, identical 2K/Ultra fixture arguments and
896 frames each (889 ordinary, seven detailed), without concurrent compilation.
Both completed without stderr. Ordinary inclusive means in ms/frame were:

| Scope | Earlier binary | Combined candidate |
| --- | ---: | ---: |
| Complete frame | 24.097 | 29.037 |
| Main M2 admission, both disjoint sites | 4.793 | 5.853 |
| Main Vulkan presentation | 13.259 | 15.539 |
| Command-recording interval, including necessary joins | 2.649 | 2.483 |
| Queue present | 8.112 | 10.147 |

The candidate separately records 0.917 ms/frame of main command capture,
0.128 ms of compositor recording and 0.599 ms of pending-scene wait. Worker scene
recording totals 1.485 ms/frame across workers; that sum is work, not critical-path
latency. Nested intervals must not be added to the complete-frame row.

This confirms worker execution but does **not** demonstrate a frame-time win.
The candidate is slower overall in this pair, including unchanged M2/UI work.
Stationary resident tile, bone, WMO and primary-shadow counts match; particle
counts and a few animated draws differ with elapsed animation time. Hidden-window
presentation, this single pair and the benchmark's below-terrain travel path
limit attribution. No live FPS gain or complete CPU-cutover completion is claimed.
Raw files use `target/cpu-group-scaling-*`, `cpu-group-trace-before-*` and
`cpu-group-trace-after-*`.
