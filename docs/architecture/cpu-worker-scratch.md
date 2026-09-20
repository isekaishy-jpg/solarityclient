# Admitted scratch per worker lane

`CpuWorkerScratch<T>` implements the worker-scratch lifetime from the CPU
composition design. One typed operation owns a bounded lane for each physical
executor worker. Compatible jobs share those lanes instead of each retaining
their own temporary container. Results continue to use separate owned buffers.

## Ownership and admission

The executor passes a private physical-worker identity to frame, loading, finite
service and resumable service operations. `JobContext` remains an operation's
scoped identity/cancellation/diagnostic view and is now explicitly non-Send and
non-Sync. Physical identity chooses temporary storage only; it does not select
gameplay RNG, callback order, priorities or result publication order.

A binding holds a weak executor identity, preventing an allocation address from
being recycled into an apparently valid owner while old scratch still exists.
`with_worker_scratch` verifies that identity, the worker index and requested
capacity. A lane's short transfer guard moves its container into an exclusive
loan and releases the guard before invoking domain code. A nested loan of the
same lane fails explicitly; no kernel waits for scratch. Normal return and unwind
clear temporaries before the return guard restores the container. A higher-ranked
callback prevents references into resettable scratch from escaping into results.

`new` charges control and lane metadata. `reserve` admits an entire replacement
version, including every worker's capacity, while the old version remains
charged. Refusal drops only the incomplete replacement. Existing clones pin their
original version through queued and running jobs. `trim` is an explicit maintenance
operation with the same old-plus-new admission contract. No reservation or hidden
allocation occurs inside the scratch callback. Per-worker peaks record the largest
declared request in that allocation generation without timestamps; these are not
process RSS or a complete account of allocations nested inside temporary values.

## Connected M2 consumer

M2 particle sorting is the first consumer. Geometry admission derives the same
particle bound from stock simulation/resource capacities, then admits worker
scratch before moving live simulation state. Each submitted geometry chunk pins
the current admitted version. A larger subsequent requirement can publish a new
version without revoking already queued work. Reclamation releases chunk pins.
Per-model jobs no longer retain individual particle-sort containers.

At the next phase's explicit admission boundary, geometry rebinds changed
executors and trims capacity exceeding twice the preceding phase's requirement.
The factor of two is allocator-retention hysteresis, not a change to stock
particle budgets, LOD, simulation or ordering. A rare larger scene therefore does
not permanently retain that capacity in every worker. Repeated views can still
cause growth and trimming; matched movement measurements must check that cost.

Camera sampling, stock clocks and RNG, particle simulation, particle sort rules,
output publication and GPU buffer lifetime remain unchanged. The existing moving
serial-geometry oracle exercises the connected path. This completes neither
domain-wide scratch adoption nor the remaining main-thread M2 distribution work.

## Validation

Formatting and all-target/all-feature workspace Clippy pass. The full workspace
suite passes 1,634 tests, with zero failures and 33 existing ignored tests across
101 suites. Four new CPU cases cover version growth/refusal/trimming with pinned
storage, foreign executor/capacity/nested-loan refusal, unwind and resumable-service
reuse, and simultaneous worker lanes followed by cancellation. Compile-fail cases
cover foreign-thread context access and escaping scratch references.

The warmed graph/fan-in allocation fixture now executes through `JobContext` and
worker scratch. It records zero allocator calls during 1,000 measured activations
on coordinator and workers. The existing moving M2 oracle still matches the serial
path through visibility changes and checks state reclamation after errors.
Logs are retained under ignored `target/worker-scratch-{clippy,test}.*.log`.

## Controlled comparison and limits

Four alternating unprofiled runs completed 16,384 frames on 2026-09-20. The
preserved Build 167 source benchmark and candidate use four CPU workers, 192
authored NPCs, the same Soap view, installed stock data, 2560 x 1440 Ultra and
the GTX 1070. Each run has 1,024 frames in each of four phases. The hidden offline
fixture excludes network, the live movement solver, sound and overlays and does
not establish desktop FPS.

| Median total frame time (ms) | Baseline 1 | Worker scratch 1 | Baseline 2 | Worker scratch 2 |
| --- | ---: | ---: | ---: | ---: |
| Matched stationary | 5.646 | 5.683 | 5.639 | 5.707 |
| Streaming | 5.559 | 5.674 | 5.556 | 5.561 |
| Orbit | 4.869 | 4.860 | 4.806 | 4.782 |
| Pointer motion | 6.462 | 6.537 | 6.491 | 6.532 |

Stationary matching requires two resident tiles, no new admission, 243 WMO draws,
482 far environment-shadow draws, 1,658 M2 draws and 23,864 bone transforms. The
matched sets contain 422, 361, 418 and 412 frames. Their p95 values are 6.647,
6.685, 6.520 and 6.667 ms. The small stationary median increases of 0.036 and
0.069 ms are recorded, not presented as a speedup or proof of unchanged timing.
Long-frame results remain mixed. One candidate's first streaming frame took
257.660 ms; the next candidate's streaming maximum was 51.635 ms.

A separate pair completed 8,192 profiled frames. Across 4,064 ordinary frames per
run, main admission is 1.843 versus 1.823 ms/frame, publication 0.563 versus
0.581 ms/frame, and CPU renderer time 1.931 versus 1.918 ms/frame. Combined main
M2 admission/publication is effectively unchanged at 2.406 versus 2.404 ms/frame.
The 32 GPU samples average 2.954 versus 2.965 ms. These observations do not
identify a material pipeline cost reduction or explain every timing difference.

Existing detail counters sample the global CPU Frame ledger at geometry finish,
not process RSS or exclusive geometry memory. Mean retained charges are
10,574,355 versus 10,548,440 bytes, a reduction of approximately 26 KB in this
fixture; maxima are 11,999,072 versus 11,970,832 bytes. Result charges are identical
(8,784,906 mean and 10,012,576 maximum bytes). This scene does not demonstrate a
large memory saving. The deterministic tests establish sharing, lifetime and
bounded growth; many small scratch demands and one large demand have different
memory tradeoffs under a worker-count bound.

The change is retained to establish the specified worker-scratch architecture,
not as a measured FPS improvement. The small stationary increase, live movement,
other scratch consumers and substantial main-thread preparation remain open.
Artifacts are under ignored `target/worker-scratch-comparison-*` and
`target/worker-scratch-profile-*`, with analysis JSON and
`target/worker-scratch-memory.json`. Executable SHA-256:

- Baseline: `748925A6D4C0BC4C62315A7F81B867AFE88585968A308C63F5EA3F741FF86F0F`.
- Candidate: `BAE9093B832D5047E14C6B9A1DC35EFCFA37BE4DBB833CD7A245479218F8D1AE`.

[Testing Build 168](testing-build168-scratch.md) packages this source boundary.
