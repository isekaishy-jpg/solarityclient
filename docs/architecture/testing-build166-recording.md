# Testing Build 166: parallel shadow recording

Build 166 packages source `4fd3ab6d0a037c2cf17fcffdae40bd57dde055f0` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 10:31 EDT. Installed and packaged executable SHA-256:

`2A8B939CC3BB254AE0A33602EF437D58A2C3E39160F546CCB911630A3F8065B2`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed and the checkout clean before compilation; tracked
source and index remained frozen until the optimized build completed.

## Included change and validation

Primary and three environment shadow passes record on the existing shared CPU
executor while main records the scene. Exclusive command pools belong to the
existing frame slots. Main keeps one ordered graphics submission, original image
dependencies and GPU fences; a scoped guard joins jobs before releasing resources
on success, failure or unwind. Required consumption services native input through
`FrameWait`. Camera sampling, gameplay publication and frames in flight are unchanged.
See the [ownership contract and complete measurements](parallel-shadow-recording.md).

- Formatting and full workspace Clippy, all targets/features with warnings denied,
  pass. Full workspace tests: 1,623 passed, zero failed, 33 existing ignored across
  99 suites. The subsequently added native-error/unwind ownership case also passes
  with all 49 rendering unit tests, for 1,624 distinct passing tests. Full shadow
  cache images agree between one and four workers through updates and quality changes.
- Four alternating optimized unprofiled runs completed 16,384 frames at 192 NPCs.
  Matched stationary median reductions are 0.62-0.74 ms; camera-motion medians also
  improve. A separate profiled pair completed 8,192 frames: mean ordinary main
  recording falls from 1.59 to 1.02 ms, including capture and pending work. Sampled
  GPU total rises slightly from 2.97 to 3.05 ms. Isolated long frames remain. These
  offline results neither establish live FPS nor meet the requested 5 ms reduction.
- Numbered compilation and `-SkipBuild` installation succeeded. Source identity,
  build number, executable hash and Testing shortcut were verified. Launch settings
  preserve four CPU workers, capacity 256, two network workers, 2560 x 1440
  fullscreen-windowed and GPU 0. No interactive client was launched.

The [remaining cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active. Main M2 admission and publication sum to roughly 2.67 ms per ordinary
frame in this profiled fixture and remain the next substantial distribution target;
whole-domain JobContext, accounting, cache/residency and service integrations also
remain required. The goal is not complete.
