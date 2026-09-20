# Testing Build 167: stable owned model jobs

Build 167 packages source `e8c2c8ebe865c123bd3e6091aea166758f6ef16d` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 14:35 EDT. Installed and packaged executable SHA-256:

`0E84FBD42D2794E4F5C2688F498F3DAA99A925F73A00CE6AE93F021C672F15AB`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed and the checkout clean before compilation; tracked
source and index remained frozen until the optimized build completed.

## Included change and validation

M2 geometry records stay in uniquely owned, budgeted cells while staging,
workers, publication and reclamation transfer their handles. Reused cells keep
their charge during main-side retention; failed executor rebinding preserves the
old owner and reuse entry. Camera sampling, stock clocks, callbacks, simulation,
draw order and contiguous renderer output remain unchanged. The
[ownership contract and measurements](cpu-owned-job-cells.md) record the limits.

- Formatting and full workspace Clippy, all targets/features with warnings denied,
  pass. Full workspace tests: 1,628 passed, zero failed and 33 existing ignored
  across 100 suites. Coverage includes real worker transfer/reclaim, failed budget
  transfer and unchanged moving geometry against the serial reference.
- Four alternating optimized unprofiled 192-NPC runs completed 16,384 frames.
  Matched stationary medians improve by 0.17-0.33 ms, with lower camera-motion
  medians. Long-frame results remain mixed, including a 258 ms first streaming
  frame in one candidate run; its exact internal cause is not established.
- A separate profiled pair completed 8,192 frames. Ordinary main M2 admission
  plus publication falls from 2.641 to 2.413 ms/frame; CPU renderer time does not
  increase. Existing Frame-ledger counters do not measure process RSS. These
  offline results do not establish live FPS or meet the requested multi-ms target.
- Numbered compilation and `-SkipBuild` installation succeeded. Source revision,
  build number, executable hash and Testing shortcut were verified. Launch settings
  preserve four CPU workers, capacity 256, two network workers, 2560 x 1440
  fullscreen-windowed and GPU 0. No interactive client was launched.

The [remaining cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active, including substantial main-thread admission/publication work,
domain-wide JobContext, working-set accounting and cache/residency integration.
This package is a validated checkpoint, not completion of the goal.
