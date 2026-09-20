# Worker-owned M2 finalization

The equipped Build 169 profile measured approximately 3.001 ms/frame in main M2
publication: 1.368 ms in geometry publication and 1.183 ms in transparent ordering.
This stage now captures completed geometry owners and the final contiguous
renderer streams into one owned CPU operation. It assembles packets and effects,
assigns the stock transparent order, and returns the same renderer-facing buffers.
The remaining serial animation/admission boundary is separate.

## Ownership and scheduling

`geometry/finalization` separates admission, capture/reclamation, streams and
ordering into focused children. The shared executor runs the operation through
`JobContext`; this introduces no thread pool. The operation owns a retained,
charged `CpuOwnedCell`, completed geometry owners, output containers, diagnostic
counters and typed scratch. It has no mutable world, camera controller, Lua or
Vulkan device borrow.

Main reclaims the terminal geometry phase and returns simulation state to the
placements before capture. Ready finalization can start between the existing
independent ground-detail/WMO steps. Receiver callbacks remain after those steps,
at the original `821A20`/`831AF0` boundary. The pure ordering work may execute
earlier; an ordering error remains deferred until after the same receiver
callbacks and lighting launch that preceded it before this change.

This is one worker operation for final assembly and ordering, alongside the
existing multi-worker model jobs. It is not a claim that sorting itself scales
across all cores. Main yields on exact phase readiness and restores all containers
on success, domain failure, worker failure or abandonment. Shader packet values,
animation/RNG order, camera inputs and renderer slice interfaces stay unchanged.

## Capacity and ordering

Completed jobs provide immutable live output counts. Main admits final vector
storage before transfer, including old-plus-new growth and executor rebinding.
The reservations live in the owned operation so an in-flight task pins them.
Retained frame vectors drop before their reservations. Nested error-string
allocations and unrelated domain storage are not newly covered by these charges.

CPU particle vertex/index storage is admitted for actual completed output rather
than unused simulation capacity. The declared particle capacities still reach
the GPU exactly as before. A larger GPU buffer requirement therefore does not
force the CPU concatenation buffer to reserve unwritten vertices.

Workers use fixed-capacity writers. The existing common transparent-key sort and
scene-order assignment remain intact. Final stable ordering sorts compact indices
by `(scene order, original ordinal)`, then applies permutation cycles to full
records. The ordinal preserves equal-key order. This replaces large-record stable
merge scratch with admitted `CpuScratch<usize>` and bounded record movement.

The earlier rejected output-copy experiment added another join while leaving
other final publication work on main. The rejected borrowed-page experiments
also changed renderer consumption. This operation owns complete assembly and
ordering and retains the original contiguous renderer API. Those differences
justify a new controlled comparison, not an assumed speedup.

## Verification status

Formatting and workspace Clippy with all targets/features and warnings denied
pass. The full workspace suite passes 1,642 tests, zero failed, 33 existing ignored,
across 101 suites. Four new tests compare compact ordering with the former stable
sort, including large records and duplicate keys, and cover capacity adoption,
growth refusal and executor transfer refusal. The moving M2 oracle additionally
abandons a finalized worker output before receiver callbacks and checks stream,
simulation-state and charge restoration. Existing lighting, equipment, mount and
retirement coverage also passes. Optimized compilation and eight controlled
4,096-frame runs also complete successfully.

## Controlled measurements

The baseline is the preserved Build 169 source benchmark executable, SHA-256
`259B3FDFD3C55B8DDE5C6B637F00185C45D4864B2B2E63B5341DD73E92C9FA8A`.
The candidate benchmark SHA-256 is
`8EE83E428963FE5DFA4011ACE9B29D5891D29789AEDA623E6BF59F05B51F4318`.
These are diagnostic executables, separate from numbered Testing packages.

Four alternating equipped runs use Soap at map 1, position
`(1515.34, -4417.27, 18.0499)`, with 300 NPCs of display 6882, main item 50732,
offhand 18805 and no ranged item. Offsets are
`(8 + (i % 12) * 1.5, -7 + floor(i / 12) * 1.5, 0)`.
The hidden fixture uses four CPU workers, capacity 256, two network workers,
2560 x 1440 Ultra and GTX 1070. Sound, live network, the live movement solver and
overlays are absent. No compiler or separate GPU test ran alongside measurement.
Each run has 1,024 frames per streaming, stationary, orbit and pointer phase.

The shared stationary tuple is three resident tiles, zero admitted tiles,
243 WMO draws, 484 far environment shadows, 3,450 M2 draws and 41,517 bones.
Selection maximizes minimum shared representation, independently of timing.

| Milliseconds | Baseline 0 | Candidate 1 | Baseline 2 | Candidate 3 |
| --- | ---: | ---: | ---: | ---: |
| Matched stationary median | 14.5946 | 13.6276 | 14.6002 | 13.7086 |
| Matched frames | 399 | 421 | 497 | 421 |
| Matched stationary p95 | 15.7631 | 14.9975 | 15.6967 | 14.8121 |
| All stationary median | 14.5776 | 13.6383 | 14.5617 | 13.7312 |
| Streaming median | 14.3665 | 13.4281 | 14.4381 | 13.5248 |
| Orbit median | 8.3838 | 8.4459 | 8.4371 | 8.6293 |
| Pointer median | 15.2382 | 14.3200 | 15.1982 | 14.4622 |

Matched gains are 0.9670 and 0.8916 ms. Orbit medians increase by 0.0622 and
0.1922 ms. Tails remain mixed: the first candidate reaches 274.6 ms during
streaming versus 74.7 ms baseline; the repeat reaches 71.5 versus 79.1 ms.
These runs do not establish hitch elimination or a universal movement gain.

The separate unarmed 192-NPC control has 606 matched stationary frames per run,
with 1,658 M2 draws and 23,865 bones; other matched counts are unchanged.
Its median increases from 5.7098 to 5.8550 ms, a **0.1452 ms regression**.
Whole stationary medians increase from 5.6996 to 5.8196 ms. The complete worker
ownership boundary and repeat equipped gain justify retaining this checkpoint,
but the lighter-scene overhead remains an open cost.

A separate equipped profile pair has 4,064 ordinary frames per executable:

| Mean milliseconds per ordinary frame | Baseline | Candidate |
| --- | ---: | ---: |
| Main M2 admission | 4.1097 | 3.8526 |
| Main M2 publication | 2.9780 | 0.2832 |
| Main finalization capture | absent | 0.0689 |
| Worker finalization | absent | 1.6485 |
| Worker assembly, nested | absent | 1.2849 |
| Worker ordering, nested | absent | 0.3613 |
| Main coordinator wait, all causes | 0.2324 | 2.2062 |
| Renderer CPU | 4.5999 | 4.5715 |
| Slot wait/upload, nested | 2.1735 | 2.1584 |
| Command recording, nested | 1.9026 | 1.8817 |

Publication is about 2.695 ms lower, but this is not the net frame saving.
Coordinator waiting increases by about 1.974 ms across all causes, and the
single finalization operation can remain on the critical path. Nested worker,
main, wait and GPU measurements must not be added as independent costs.
Main admission and renderer work remain substantial distribution targets.

Raw local evidence is retained under `target/finalization-equipped-*`,
`target/finalization-unarmed-*`, `target/m2-finalization-comparison-analysis.json`
and `target/finalization-equipped-profile-analysis.json`. These offline results
do not establish live FPS or complete the requested multi-ms improvement.

The [complete cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active, including main-thread admission, other domain adapters and complete
working-set accounting.
