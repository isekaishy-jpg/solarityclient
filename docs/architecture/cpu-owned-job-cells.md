# Stable owned job cells

The CPU cutover requires bounded, reusable job ownership without moving large
domain records through every scheduler and publication boundary. `CpuOwnedCell`
admits one value before calling its initializer. Moving the cell transfers the
existing allocation and its unique charge; it does not copy the contained value.
The owner can borrow that value mutably, while ordinary Rust ownership prevents
concurrent producer, worker and consumer access. This introduces no new unsafe
code, worker phase, blocking wait, shared mutable job or runtime fallback.
An explicit budget transfer preserves the allocation and value; a rejected
destination preserves the original charge. Reuse performs that transfer before
removing an owner from its generation list, allowing executor rebinding without
losing the original owner on admission failure.

M2 geometry is the first connected consumer. `GeometryOwner` keeps its complete
job in a cell; staging groups and reclamation move only owners. The existing
generation/visible/shadow reuse key still selects storage, and unmatched owners
retire after the phase finishes. Worker result publication still produces the
existing contiguous renderer arrays. The rejected borrowed-output experiment is
not reinstated.

The record's charge now survives reclamation and main-side retention. Previously
worker chunks charged their inline record capacity, but the completed main-side
record vector was ordinary unaccounted storage. Nested pose, simulation and other
domain allocations still need their own adoption; this cell is not a claim of
complete domain accounting. Chunk capacity now charges owner handles, while the
one payload allocation is charged independently throughout its actual lifetime.

Stock clocks, callbacks, RNG, admission, effects, draw order and camera/input
sampling remain in their existing places. This changes owned storage, not gameplay
or work requirements. The established moving serial-geometry oracle is retained.

Formatting and all-target/all-feature workspace Clippy pass. The full workspace
suite passes 1,628 tests with zero failures and 33 existing ignored tests across
100 suites. New CPU tests cover rejected admission before construction, budget
transfer and stable payload addresses through real execution, result consumption
and repeated reclamation. Existing M2 fixtures exercise reuse
under moving visibility, generation changes, submission refusal, abandoned frames
and geometry errors. The moving geometry oracle matches the frozen serial path.
Logs are under ignored `target/m2-cells-{clippy,test}.*.log`.

## Controlled optimized comparison

Four alternating baseline/candidate runs completed 16,384 unprofiled frames on
2026-09-20. Both executables use four CPU workers, 192 authored NPCs, the same
Soap camera fixture, installed stock data and 2560 x 1440 Ultra settings on the
GTX 1070. Each run contains 1,024 frames in each of streaming, stationary,
orbit and pointer-motion phases. The hidden offline fixture excludes network,
the live movement solver, sound and overlays; it does not establish desktop FPS.
The unchanged baseline is the preserved Build 166 benchmark executable.

| Median total frame time (ms) | Baseline 1 | Cells 1 | Baseline 2 | Cells 2 |
| --- | ---: | ---: | ---: | ---: |
| Matched stationary | 5.928 | 5.594 | 5.829 | 5.661 |
| Streaming | 5.886 | 5.558 | 5.855 | 5.553 |
| Orbit | 5.103 | 4.726 | 5.072 | 4.834 |
| Pointer motion | 6.949 | 6.415 | 6.677 | 6.583 |

Stationary matching requires two resident tiles, no new admission, 243 WMO draws,
482 far environment-shadow draws, 1,658 M2 draws and 23,864 bone transforms;
the four matched sets contain 410, 370, 410 and 416 frames. Their p95 values are
7.044, 6.526, 6.803 and 6.722 ms. These runs support a modest 0.17-0.33 ms
steady median reduction, not the user's requested multi-ms overall improvement.
Long-frame results remain mixed. The first candidate streaming frame took
258.382 ms, of which 244.915 ms was presentation; this unprofiled sample does
not establish its internal cause. The second candidate's streaming maximum was
49.828 ms, while its pointer phase still contained a 53.826 ms frame.

A separate profiled pair completed 8,192 frames. Across 4,064 ordinary frames per
run, main M2 admission falls from 1.931 to 1.853 ms/frame and publication from
0.710 to 0.560 ms/frame. CPU renderer time is 1.932 versus 1.911 ms/frame;
there is no observed transfer of the saving into a larger renderer cost.
The 32 GPU samples average 2.963 versus 2.898 ms, too sparse to attribute a GPU
improvement to this CPU storage change. Existing long-frame and remaining main
admission/publication distribution work are not resolved by these measurements.

Existing detail counters sample the global CPU Frame ledger at geometry finish,
not exclusive geometry usage or process RSS. Their mean retained charge is
11,095,072 versus 10,573,346 bytes, with maxima 23,271,762 versus 12,000,096.
Result charges are nearly identical (8,784,906 versus 8,784,463 mean bytes;
10,012,576 maximum in both). Different streaming timing and calibrated chunk
counts limit this comparison; the deterministic ownership tests establish the
new record charge's lifetime. This is not evidence of an equivalent RSS reduction.

Artifacts remain under ignored `target/m2-cells-comparison-*` and
`target/m2-cells-profile-*`, with JSON analysis and `target/m2-cells-memory.json`.
Baseline SHA-256 is
`CDC77269CC13A7E62D4219AB9ADCD1A3527671321A58673EBE30E161A32BDFF6`;
candidate SHA-256 is
`748925A6D4C0BC4C62315A7F81B867AFE88585968A308C63F5EA3F741FF86F0F`.
No new numbered Testing build has been installed at this source checkpoint.
