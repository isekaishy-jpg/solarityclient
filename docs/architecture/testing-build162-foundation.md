# Testing Build 162: execution policy and job context

Build 162 packages source `eac55b9f1ce8582b8e98d2eae0a1690fb598ffc4` on
`perf/critical-frame-work`. It was installed through the existing Testing shortcut
on 2026-09-20. Installed and packaged executable SHA-256:

`11515C222611EB51078D68F97B0275063A49CB10B62C3309A0E4C0D75DA18EF3`

The package reports `dirty=true` because reserving its build number changes
`BUILD_NUMBER`; source and tests were committed before packaging. Tracked source
and the git index stayed frozen throughout compilation.

## Connected changes

- Current placement ownership follows storage mutations before render metadata
  publication. Dynamic unit lookup avoids repeated full-scene searches. Unit
  residency, removal and state updates now have focused folder modules.
- Runtime resolves a validated execution plan for protected/flexible workers,
  service priority and concurrent bulk capacity. The executor enforces that plan.
- Contextual frame and loading kernels receive admission provenance, inherited
  diagnostics, cooperative withdrawal and preadmitted scoped scratch. M2 geometry
  and dependent model loading use these interfaces; particle sorting uses the
  scratch boundary.

See [the adoption record](cpu-cutover-foundation-adoption.md) for exact contracts
and remaining limits. The camera-clock and geometry-storage fixes from Builds
160 and 161 remain. The user confirmed smooth camera movement and no crash in
Build 161 before this checkpoint.

## Validation

- Formatting and full workspace Clippy, all targets/features with `-D warnings`, pass.
- Full workspace tests pass: 1,611 passed, zero failed, 33 existing ignored tests.
- The hidden 896-frame Orgrimmar replay completed all seven phases with zero
  profiling drops or capacity overflows. Seven detail samples reached 12,580,610
  geometry Frame bytes and 4,698,136 Result bytes. These are sampled retained
  charges, not global peaks or whole-process memory usage.
- Numbered optimized compilation and `-SkipBuild` installation succeeded.
  Installed revision, build number, executable hash and shortcut were verified.
- Installation retains four CPU workers, capacity 256, two network workers,
  2560 x 1440 fullscreen-windowed and GPU 0. No interactive client was launched.

The hidden fixture omits live networking, movement solving, audio and overlays.
This package establishes no live FPS improvement and does not complete the CPU
cutover. Remaining work includes scheduler publication/measurement, typed products,
nested memory accounting, finite service boundaries and further domain adoption.
