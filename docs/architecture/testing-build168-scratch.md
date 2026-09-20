# Testing Build 168: admitted worker scratch

Build 168 packages source `5ae0207cc3b7c89c3293e093ca2b8919714b7512` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 15:17 EDT. Installed and packaged executable SHA-256:

`B7DA71AD967FBC423D5462A8049D774D147B8D6270288FC2914972CD25E2F96A`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed and the checkout clean before compilation; tracked
source and index remained frozen throughout the numbered build.

## Included change and validation

Frame, loading and service operations receive physical execution identity through
their scoped `JobContext`. The new typed worker-scratch binding preadmits bounded
lanes, retains old versions during growth/trimming, and returns empty storage on
success and unwind. M2 particle sorting uses this shared storage. Camera sampling,
stock clocks, particle simulation, sorting and publication order stay unchanged.
`JobContext` is explicitly non-Send/non-Sync; separate work receives owned inputs
and copyable provenance instead. See the [contract and complete measurements](cpu-worker-scratch.md).

- Formatting and full workspace Clippy, all targets/features with warnings denied,
  pass. Full workspace tests: 1,634 passed, zero failed, 33 existing ignored across
  101 suites. Moving geometry matches the serial reference. New coverage checks
  ownership/refusal/unwind/cancellation and compile-time lifetime/affinity limits.
- The warmed graph/fan-in test uses worker scratch and records zero allocations
  across 1,000 measured activations on coordinator and workers.
- Four alternating unprofiled 192-NPC runs completed 16,384 frames. Matched
  stationary medians rise by 0.036-0.069 ms; orbit medians are slightly lower.
  Long frames remain. There is no demonstrated FPS gain from this change.
- A separate profiled pair completed 8,192 frames. Combined main M2 admission and
  publication is effectively unchanged at 2.406 versus 2.404 ms/frame. The global
  CPU Frame ledger retains approximately 26 KB less on average in this fixture;
  this does not establish a significant RSS reduction. The NPC fixture has no
  equipment attachments and is not a complete mixed-population workload.
- Numbered optimized compilation and `-SkipBuild` installation succeeded. Source
  revision, build number, executable hash and Testing shortcut were verified.
  Settings preserve four CPU workers, capacity 256, two network workers,
  2560 x 1440 fullscreen-windowed and GPU 0. No interactive client was launched.

This is a required architecture checkpoint, not the requested multi-ms performance
fix. Main-thread preparation, equipped-crowd qualification, other domain adapters,
working-set admission and the [remaining cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active. The goal is not complete.
