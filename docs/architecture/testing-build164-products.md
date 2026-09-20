# Testing Build 164: typed dependency products

Build 164 packages source `d17673dbf32f8a4300b99039cd98500b5099b574` from
`perf/critical-frame-work`. Installed through the Testing shortcut on 2026-09-20
at 08:03 EDT. Installed and packaged executable SHA-256:

`7A5AB614E30687A1857E2ED8AA5C10BB6B3C00BE7FDE9228B50780B3091E910B`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed before compilation; tracked files and the index
remained frozen until the optimized build completed.

## Included changes and validation

- Immutable CPU products bind result ownership and dependency readiness to the
  same generation. A unique publisher stores the typed result before waking
  dependents; abandonment, errors and result lifetime remain explicit.
- M2 appearance/GameObject loading dependencies use these products while
  retaining the original resource lease and consumer urgency. Polling no longer
  needs the separate request-result mutex. See [the ownership contract and
  remaining limits](cpu-shared-products.md).
- Formatting and full workspace Clippy, all targets/features with warnings
  denied, pass. Full workspace tests: 1,620 passed, zero failed, 33 existing
  ignored across 99 suites.
- Optimized numbered compilation and `-SkipBuild` installation succeeded.
  Build identity, executable hash and Testing shortcut were verified. Launch
  settings preserve four CPU workers, capacity 256, two network workers,
  2560 x 1440 fullscreen-windowed and GPU 0. No interactive client was launched.

The camera correction is preserved. This package completes an ownership
checkpoint, not the remaining main-thread preparation distribution or a measured
FPS improvement. The [fresh Build 163 capture](live-build163-brewfest.md) locates
that remaining cost, with its workload/comparison limitations recorded.

The user clarified that 200–300 mixed players/NPCs/creatures/critters is a normal
live-server workload. Remaining distribution takes priority over completing
unrelated JobContext adapters: M2 geometry already receives JobContext. Broader
character batching and the remaining individual primary-shadow submissions are
separate renderer work; compatible adjacent M2/environment-shadow instancing
already exists.
