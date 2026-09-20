# NPC population scaling during the CPU cutover

The 2026-09-20 Brewfest report led to an isolated population sweep through the
existing optimized `benchmark_world` example. This reproduces a substantial
population-dependent frame cost, not the exact live Brewfest encounter. It is a
measurement checkpoint, not an NPC performance fix.

## Fixture and controls

- Production source matches `f18e99fe`; compiled before that commit, so the
  diagnostic executable reports `415fc671`, dirty. It is not installed Build 162.
- Executable SHA-256:
  `D875842818CF89CDC37BA2B05D9E095A316E96D4B6B219889528D2AFC3AF081F`.
- Optimized `test-client` profile, hidden 2560 x 1440 Vulkan surface, GPU 0,
  four CPU workers, capacity 256, two network workers, VSync 0, shadow quality 5.
- Soap at map 1, `(1515.34, -4417.27, 18.0499)`, unchanged default camera and
  realm hour. Added NPC display 6882, no equipment, on the same grid:
  `dx = 8 + (index % 12) * 1.5`, `dy = -7 + floor(index / 12) * 1.5`, `dz = 0`.
- Population order: 0, 48, 192, 48, 0. Each run has 1,024 frames per phase:
  streaming, stationary, orbit and pointer motion. F10-equivalent profiling is
  enabled with detail every 128th frame. No framebuffer capture.
- The initial sweep overlapped rendering tests and is discarded for timing.
  The reported sweep started after all workspace tests and builds exited.

The fixture uses actual asset loading and rendering but omits live network
traffic, movement solving, audio and overlays. A hidden surface is not a desktop
FPS comparison. The frames are wall-clock animated, not identical animation ticks.

## Matched stationary samples

Streaming advances over wall time, so comparing all stationary frames would
include different resident tile counts. The table selects stationary frames with
exactly two resident tiles and no admitted tile that frame. All selected frames
have 243 WMO draws, zero terrain/low-detail/ground-detail draws and 482 far-shadow
draws. NPC mesh and near/primary shadow counts intentionally vary with population.

| Added NPCs | Selected frames | Median total frame time |
| --- | ---: | ---: |
| 0, first run | 528 | 2.8355 ms |
| 48, first run | 444 | 3.8984 ms |
| 192 | 388 | 7.3515 ms |
| 48, repeat | 466 | 3.9371 ms |
| 0, repeat | 500 | 2.7729 ms |

The 192-NPC case adds about 4.5 ms relative to the first zero-NPC case. Stationary
visible M2 draw counts are 130, 514 and 1,658 for 0, 48 and 192 added NPCs.
The lower-count repeats return close to their initial timings, supporting a
population-related cost rather than a monotonic slowdown over these five runs.
This does not rule out the separately reported long-run degradation.

## Where the added work appears

These are mean **ordinary-frame** CPU-side spans over all four phases, not the
matched stationary subset above. Totals include nested work, yields and timing
overhead; do not add them or interpret them as exclusive CPU execution/GPU time.

| Span | 0 NPCs, first run | 192 NPCs |
| --- | ---: | ---: |
| M2 scene admission | 0.198 ms | 0.636 ms |
| M2 placement admission, all resumptions per frame | 0.839 ms | 2.072 ms |
| M2 geometry publication | 0.106 ms | 0.652 ms |
| Vulkan world presentation | 1.044 ms | 2.594 ms |
| Vulkan command recording | 0.600 ms | 1.601 ms |
| Vulkan slot wait and upload | 0.204 ms | 0.655 ms |

This is evidence against treating the hotspot as only root-pose preparation or
only worker scheduling. Root poses already run on workers. Ordered placement,
final stream publication, CPU upload preparation and draw command volume remain
population-scaled consumers. The roughly 1 ms increase in command recording is
particularly relevant to the separate batching/instancing workstream. It does not
establish that one proposed change will recover the full 4.5 ms.

The next cutover work should remove serial consumption/copy boundaries where
typed ownership permits it and account for retained pose/final-stream storage.
Draw batching and character instancing remain distinct renderer work, with stock
material, attachment, shadow and traversal behavior still required.

Raw artifacts are ignored under `target/npc-density-clean-*`: five frame CSVs,
profiling summaries/traces and stdout/stderr logs. The executable hash and helper
arguments are retained with the study. The contaminated initial artifacts use
`target/npc-density-*` without `clean` and must not be used as a baseline.
