# Rejected receiver-only worker phase

On 2026-09-20 an owned unit floor-query prototype was tested against the current
`ee3d3b43` source. It was **removed**, not installed or accepted as a performance
improvement. Testing Build 164 remains unchanged.

## Boundary tested

After geometry publication, the prototype captured demanded unit floor-cache
misses into independent CPU groups. Inputs pinned exact WMO source generations
and both independently stored placement matrices. Each group owned mutable BSP
scratch. Main consumed results in receiver order, retaining the original light
transition clocks and excluding GameObjects' distinct MapObject registration.

The optimized prototype compiled. Two focused tests passed: cached and uncached
results matched the existing native-floor implementation, and a worker retained
the captured floor after removal of that root from the live coordinator. These
tests do not establish complete integration correctness or performance.

## Controlled comparison

The earlier population-study executable predates other cutover changes, so a
fresh baseline was compiled from `ee3d3b43`. The candidate differs by the
prototype only. Both used the optimized `test-client` profile. No compilation
or GPU tests overlapped the runs.

- Baseline SHA-256: `AFD5A941E97A6F2062A0D843560F52FD7F3875E044EB8FB8B682696CDD306680`.
- Candidate SHA-256: `9003C4D678ED49AE5ACED86BF220D739C108DABC530AD79DF6DF08EF3B3470F1`.
- Run order: baseline, candidate, baseline; 4,096 frames each, 12,288 total.
- Same [192-NPC population fixture](npc-density-cutover-measurements.md): Soap,
  location, camera, 2560 x 1440 hidden surface, Ultra shadows, four CPU workers,
  VSync disabled. Each run includes streaming, stationary, orbit and pointer
  motion phases. Profiling is enabled in all three runs.
- Stationary matching requires two resident tiles, no tile admission that frame,
  243 WMO draws and 482 far-shadow draws. All selected frames have 1,658 M2 draws
  and 23,864 bone transforms. Animation is wall-clock driven, not an identical
  sequence of animation timestamps.

| Run | Matched frames | Median frame | Matched p95 |
| --- | ---: | ---: | ---: |
| Baseline first | 364 | 7.078 ms | 8.155 ms |
| Candidate | 327 | 7.158 ms | 9.025 ms |
| Baseline repeat | 393 | 7.083 ms | 8.209 ms |

The matched subset alone understates the failure. The candidate's full-run
frames reached 3,215.645 ms, versus 260.337 ms and 65.554 ms in the two baselines.
Candidate pointer-phase mean frame time rose to 28.732 ms despite a 7.879 ms
median. Neither the medians nor the severe tails justify accepting the change.

## What the trace and code establish

Ordinary-frame main receiver preparation fell only from 0.109/0.113 ms in the
baselines to 0.086 ms. New main query capture added 0.048 ms. These are whole-run
scope totals divided by ordinary frame count, not matched-subset measurements.
The existing receiver timer also includes demand selection, native light-state
updates, receiver storage and draw-index remapping. It is not a BSP-only timer.

The new worker-query scope accumulated 33.765 ms per ordinary frame **summed
across workers**, with individual kernels reaching 142.675 ms. Main's necessary
reclamation wait reached 3,201.696 ms. Summed worker CPU-side intervals cannot be
added to frame wall time; they identify substantial work introduced by this
prototype, rather than a useful redistribution of the former receiver scope.

Code inspection explains why the ownership boundary was unsuitable:

1. `prepare_frame_scene` can already populate unit registration through
   `unit_scene_admits`; later `EntityLighting::scene_state` shares the same cache.
   A receiver-only phase often arrives after the expensive selection is done.
2. Capturing only decoded WMO leases made each group's first root reconstruct a
   complete `PlacedWorldModelCollision`. That constructor derives all groups'
   cached-leaf eligibility and local liquid meshes, as well as placed bounds.
   These immutable derived tables were not shared from the existing collision
   generation. Shrinking/repartitioning groups could repeat that construction.
3. The phase therefore added ownership, capture and a required dependency while
   duplicating preparation far beyond the requested floor interpolation. The
   timing identifies the expensive query kernels; attributing their individual
   instructions requires further sampling, not assumptions about lock contention.

## Consequence for the cutover

Do not revive this late receiver-only phase as the answer to the multi-ms main
thread cost. Distribution must start before the first expensive consumer and
share prepared immutable spatial data while retaining exclusive query scratch.
It must cover a meaningful preparation product and its consumers, including
ordered assembly, rather than add a worker handoff after main has populated its
caches. Callbacks, RNG, attachment ordering and camera behavior remain required.

The complete cutover remains active. This experiment supplies a rejection and
stronger constraints; it is not an NPC fix, an FPS gain, or completion of a
foundation workstream.

Local raw artifacts are retained under ignored `target/receiver-query-comparison-*`.
The candidate's 17 source files and hashes are preserved in
`target/receiver-query-source-snapshot/manifest.json`; both executable artifacts
remain local. All prototype changes were removed from the working source after
comparison. No new numbered Testing package was created from them.
