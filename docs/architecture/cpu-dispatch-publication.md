# Batched publication and scheduler observation

`Core::launch` publishes all reserved runners through one dispatcher acquisition
and one notification after publication. Queue classification still reads only
atomic urgency/cost and service identity. No phase lock or domain callback runs
under the queue lock. Existing runner reservations, cost/FIFO ties, service
eligibility and ordered result reclamation remain authoritative.

The central queue is retained. Removing repeated enqueue locks is not evidence
that a lock-free scheduler would improve this client, or that scheduling is the
dominant live frame cost.

## F10 intervals

New observations start only on selected detail frames. Unselected operations
read no clock and allocate no observation records; an already selected interval
still captures its endpoint if it finishes later. Fixed optional
timestamps in queue records and worker sleep slots are covered by startup
metadata admission. The guard wrapper retains a completed acquisition interval
until unlock; publishing it never extends the measured acquisition. Publication
wakes workers before emitting the publisher's lock observation.

| Metric | Meaning |
| --- | --- |
| `cpu.dispatch.lock_wait` | Queue mutex acquisition, including uncontended acquisition overhead; excludes ownership duration and condition parking. |
| `cpu.batch.lock_wait` | Batch ownership-mutex acquisition, with the same exclusions. |
| `cpu.dispatch.frame.queue_wait` | Frame runner publication through removal from its eligible queue. |
| `cpu.dispatch.service.queue_wait` | Service runner publication through removal, including eligibility/priority delay. |
| `cpu.dispatch.priority.queue_wait` | Metadata-priority runner publication through removal. |
| `cpu.dispatch.protected.wake` | First notification of a parked protected worker through native return and mutex reacquisition, when that wake finds eligible work. |
| `cpu.dispatch.flexible.wake` | The same interval for flexible workers. |

Queue timestamps survive promotion and reclassification. Resumed service work
receives a new queue interval. Queue removal is not the first instruction of a
domain kernel: a runner may still need its batch lock or discover that another
runner consumed the remaining work. Existing kernel spans measure execution.
These intervals overlap; adding their totals is not a frame-time calculation.

Wake observations exclude idle time before notification and do not reset when
another publisher notifies the same still-parked worker. Spurious or ineligible
wakes that return to parking are not included in useful-wake distributions.
Shutdown is not a useful wake. This is notification-to-reacquisition latency,
not an OS-only scheduling measurement. Observer overhead remains nonzero on
detail frames, and old capture generations are rejected at publication.

## Verification fixture

`dispatch_scaling` runs 32 independent kernels at 1/3/5/7 workers, with empty and
fixed deterministic arithmetic kernels. It repeats disabled/enabled/disabled
capture three times and separately exercises immediate resubmission and a
requested 1 ms idle gap. Frame timing excludes the gap. The OS may sleep longer;
only actual native parking produces wake records. Every input is checked for
exactly-once execution. This fixture is scheduler evidence, not live FPS or a
whole-process concurrency recommendation.

Baseline and candidate use isolated Cargo target directories. An initial shared
target attempt produced identical executable hashes because Cargo reused an
artifact across worktrees; that attempted comparison is invalid and discarded.
The reproducible baseline is Build 162 source `39e36d2f` with the same example
added. Preserve executable hashes and capture metadata with reported results.

## Optimized comparison, 2026-09-20

The valid comparison used the `test-client` profile, isolated targets, and three
repetitions of each worker count/mode. A second pass reversed baseline/candidate
order and added a publication-only diagnostic variant to distinguish batching
from observer cost. These are medians of per-run p50 phase durations, in
microseconds, for immediate resubmission with capture disabled:

| Workers | Kernel | Build 162 | Publication only | Publication + observation |
| --- | --- | ---: | ---: | ---: |
| 1 | Empty | 11.20 | 11.60 | 13.10 |
| 3 | Empty | 19.80 | 21.50 | 22.25 |
| 5 | Empty | 26.60 | 34.05 | 34.40 |
| 7 | Empty | 31.90 | 37.45 | 34.85 |
| 1 | 20,000 iterations per kernel | 1239.15 | 1241.65 | 1237.40 |
| 3 | 20,000 iterations per kernel | 447.45 | 437.00 | 447.05 |
| 5 | 20,000 iterations per kernel | 311.25 | 308.20 | 321.30 |
| 7 | 20,000 iterations per kernel | 282.20 | 288.45 | 308.30 |

Batch publication wakes the admitted width together, which increases contention
for empty kernels at larger widths. The publication-only variant shows that
this accounts for most of the five-worker empty-kernel increase. The disabled
observation path still has branch/metadata costs; it is not free. Substantive
work scales, but run variation and slower candidate cases prevent claiming an
improvement over Build 162 or recommending a new installed worker count.

Candidate capture-enabled medians for those same substantive cases were
1246.1/439.6/303.4/300.6 microseconds at 1/3/5/7 workers. Capture enables existing
instrumentation too; these differences are not an isolated measurement of the
new observer. Detail frames retain their extra cost, rather than subtracting an
assumed overhead from results.

The second candidate pass recorded 7,902 batch acquisitions averaging 341 ns,
2,171 dispatcher acquisitions averaging 123 ns, and 1,083 frame-runner queue
intervals averaging 15,034 ns. Useful wake averages were 10,602 ns (53 flexible
samples) and 26,109 ns (157 protected samples); their maxima were 44,000 and
474,400 ns. All twelve captures reported zero drops/overflows. These aggregate
intervals overlap and include OS scheduling effects; maxima are not steady cost.
Per-thread p50/p95/p99 histogram bounds remain in the summary CSVs, while the
example prints measured whole-phase percentiles separately.

Ignored evidence: `target/dispatch-*-v3-results.log`,
`target/dispatch-*-v3-profile/Profiles`, and
`target/scheduler-benchmark-v3-summary.json`. Executable SHA-256 identities:

- Baseline: `0E7B1B6F9EB078CB1EDF1329E3EE17E7603F65B460B776B9C0BBC900B0B5AE50`
- Publication only: `7D2D2C7680F2D25E60D65816A0CB595E99895C0B149B3343FD260D6956FD9BF1`
- Candidate: `650DF8F3E51DB90B5B8B102CC5D1A1F3C0ED1A0320A41871C7FE3134D452797F`

This supports retaining the central scheduler and its measurement boundary.
It does not close matched live movement/loading, whole-process scaling, or
fine-grained causal product attribution. No Testing package or FPS improvement
is established by this synthetic comparison.

## Correctness validation

Formatting and full-workspace Clippy (all targets/features, `-D warnings`) pass.
Full workspace tests pass: 1,613 passed, zero failed, 33 existing ignored tests.
New controlled gates verify that all reserved runners enter at 1/3/5/7 workers
before release, across repeated epochs, and that all owned inputs return.
Capture coverage exercises frame and resumable-service queue observations and
separate ownership acquisition. Existing cost/FIFO, priority, service fairness,
cancellation, shutdown, zero-allocation warmed graph/service and moving M2 parity
coverage remains green. Logs are `target/scheduler-publication-final-*`.
