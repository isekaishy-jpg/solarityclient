# Testing Build 165: worker-owned palette upload

Build 165 packages source `bad0038e63d87fdbcd7e3709765950113592d8e1` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 09:37 EDT. Installed and packaged executable SHA-256:

`4F434B62BDBA63C325B95E6DBEAE0190366A0D7DDEF17C4E1148A846B399D0B2`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed and the checkout clean before compilation; tracked
source and index remained frozen until the optimized build completed.

## Included change and validation

M2 geometry palettes now remain in completed worker jobs through synchronous
renderer upload. Main publication keeps checked logical offsets without copying
a second palette, and GPU upload copies complete palette pages. Both world and
login-model rendering use the new source contract. Stock bone math, draw order,
shadow-only selection, sky offsets, camera/input order and GPU fences remain
unchanged. Resource upload and palette contracts have focused folder modules.
See [ownership, evidence and limits](m2-palette-pages.md).

- Formatting and full workspace Clippy, all targets/features with warnings
  denied, pass. Full workspace tests: 1,623 passed, zero failed, 33 existing
  ignored across 99 suites, including exact wire bytes and the frozen serial
  geometry comparison during motion, visibility changes and abandonment.
- Four optimized unprofiled hidden runs completed 16,384 frames with the same
  192-NPC fixture. Matched stationary medians improve by 0.3782 and 0.5330 ms;
  moving-camera medians also improve. Outliers and fixture limitations are
  recorded in the linked report. These are offline results, not live FPS gains
  or the requested multi-ms completion of model preparation distribution.
- Numbered compilation and `-SkipBuild` installation succeeded. Source identity,
  build number, executable hash and Testing shortcut were verified. Launch
  settings preserve four CPU workers, capacity 256, two network workers,
  2560 x 1440 fullscreen-windowed and GPU 0. No interactive client was launched.

The [rejected late receiver-query prototype](m2-receiver-query-experiment.md)
is absent. The [remaining cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active, including main admission and further output assembly, working-set
accounting and remaining domain integration. The modern Classic binary remains
an additional pinned reference; 3.3.5 remains the gameplay parity authority.
