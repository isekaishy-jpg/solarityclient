# Testing Build 161: geometry output retention

Build 161 packages source `3240076886910bc1d5e30b4e1de218f8544d24c3` on
`perf/critical-frame-work`. It was installed into the existing Testing shortcut
on 2026-09-20. Installed and packaged executable SHA-256:

`BEA46A5F5A40D7D023B3EAEF2B64D4BEC128E3198B28ECB8848A1C62D1BDCA00`

The package reports `dirty=true` because reserving its build number changes
`BUILD_NUMBER`; source and tests were committed before packaging. Tracked source
and the git index stayed frozen throughout compilation.

## Change and evidence

The preceding live run ended with a CPU Frame storage refusal: 27,216 bytes
requested with 23,432 bytes remaining under the 128 MiB limit. Geometry jobs
reused by traversal ordinal could retain large particle capacities after a
different model occupied that ordinal. Camera/visibility changes could spread
those allocations across the retained job list.

Build 161 reuses preceding-frame output buffers by source generation and
visible/shadow demand, and retires unmatched buffers during reclamation. The
budget is unchanged. Build 160's clock-dependent camera sampling fix remains.
See [the retention analysis](m2-output-storage-reuse.md) for ownership rules,
counter meanings and the limits of attribution to the original exit.

## Validation and limits

- Formatting and full workspace Clippy, all targets/features with `-D warnings`, pass.
- Full workspace tests pass: 1,603 passed, zero failed, 33 existing ignored tests.
  This includes order-rotation/budget regressions and moving worker/serial
  geometry parity with effect-state return.
- The hidden 896-frame Orgrimmar replay completed. Seven detail samples reached
  12,580,168 Frame bytes and 3,790,872 Result bytes, with no profiling drops.
  These are sampled values in a different scene, not whole-run allocation peaks
  or a matched live FPS comparison. The baseline replay also completed.
- Numbered `test-client` compilation and `-SkipBuild` installation succeeded;
  installed source revision, build number, executable hash and shortcut match.
- Installation retains four CPU workers, capacity 256, two network workers,
  2560 x 1440 fullscreen-windowed and GPU 0. No interactive client was launched.

The live scene that exhausted storage has not been reproduced exactly. The
identified retention defect is fixed; remaining camera hitches and live storage
pressure still require observation. This package does not complete the CPU
cutover or establish an FPS improvement.
