# Build 169 live capture, 2026-09-20

The user supplied `capture-1789936782127-1`, recorded at 16:39:42-16:41:19 EDT
under the Testing `Profiles` directory. Metadata identifies Build 169, revision
`75a21fcab862ce6c7e9bb7f52441612c09081ae5`, four CPU workers and two network
workers. Duration is 97.699 seconds: 11,847 ordinary frames and 94 detail frames.
Trace/event drops and capacity overflows are zero; two metric samples dropped.
The normal client log ends with `UiQuitRequested`, not a reported crash.

## Limits

Build 170 compilation ran from approximately 16:38:58 to 16:44:32 EDT,
overlapping the entire capture. These timings are not an isolated performance
comparison. The route/camera were not recorded as a matched fixture. Detail
frames are excluded from ordinary aggregates. Observer overhead is not subtracted.

## Main work and distribution

Ordinary frame mean is 8.163 ms. Main admission totals 2.674 ms/frame across
two non-overlapping call sites: initial scene preparation (0.885 ms) and resumable
placement admission (1.789 ms). Main publication is 0.830 ms. Their combined
3.504 ms is about 43% of frame time. Renderer CPU is 1.373 ms and session/world
service 1.584 ms. Nested scopes are not additional frame costs. GPU time averages
2.759 ms across 94 samples and is not added to CPU durations.

| Capture seconds | Frames | Frame mean ms | Admission ms/frame | Publication ms/frame | Renderer ms/frame | Mean M2 draws |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0-10 | 828 | 10.947 | 3.798 | 1.781 | 1.934 | 1018 |
| 10-20 | 819 | 12.333 | 4.302 | 1.965 | 2.127 | 1139 |
| 20-30 | 1066 | 9.460 | 3.131 | 0.966 | 1.459 | 528 |
| 30-40 | 1289 | 7.839 | 2.730 | 0.644 | 1.226 | 266 |
| 40-50 | 1542 | 6.555 | 2.078 | 0.428 | 1.130 | 186 |
| 50-60 | 1484 | 6.133 | 1.959 | 0.392 | 0.947 | 123 |
| 60-70 | 1586 | 6.380 | 1.986 | 0.502 | 1.021 | 255 |
| 70-80 | 1293 | 7.815 | 2.511 | 0.818 | 1.505 | 590 |
| 80-90 | 1065 | 9.484 | 3.076 | 1.081 | 1.754 | 789 |
| 90-end | 875 | 8.935 | 2.708 | 0.728 | 1.376 | 385 |

Windows use completed one-second snapshots. The busiest interval attributes
6.267 ms of its 12.333 ms mean to M2 admission/publication. This establishes a
substantial recurring boundary, not a specific inner cause for all of its cost.

OS thread deltas show main consuming 0.955 of one core. The three frame workers
consume 0.090, 0.091 and 0.089 cores; the flexible worker consumes 0.133.
Execution is connected but remains strongly concentrated on main. Concurrent
compilation prevents an isolated scheduler-efficiency conclusion.

Working set starts at 2,155 MB, peaks at 2,313 MB and ends at 1,077 MB. Private
bytes start at 3,300 MB, peak at 3,461 MB and end at 2,172 MB (decimal MB).
Scene changes and the quit boundary prevent a steady-scene retention comparison;
the capture does not demonstrate monotonically increasing memory.

## Long frames and next boundary

The largest frame is 315.066 ms at completion frame 11,940, the final captured
frame. Session/world service takes 304.589 ms, including 304.500 ms in
`session actions and transfers`. The log records quit at that boundary. The
scope covers several operations; its precise inner cause is not established.
This terminal-frame cost is separate from ordinary M2 performance. Other largest
ordinary frames reach about 24.6-25.4 ms, so gameplay tails remain open.

[Build 170](testing-build170-finalization.md) moves complete final assembly and
ordering into an owned operation. This Build 169 capture does not measure that
code. Main admission remains a substantial target. Obtain uncontended, matched
runs before claiming a live build-to-build improvement.
