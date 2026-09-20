# Testing Build 160: camera clock sampling

Build 160 packages source `dcd1e4e47372ba88e3b13ecece6589e1f7a75a81` on
`perf/critical-frame-work`. It was installed into the existing Testing shortcut
on 2026-09-20. Installed and packaged executable SHA-256:

`A24C538B24B3F5EA3CA63781905CB93CF1EA324368C561240AD4ADB88EC088FB`

The package reports `dirty=true` because reserving its build number changes
`BUILD_NUMBER`; the camera source and tests were committed before packaging.
Tracked source and the git index stayed frozen throughout compilation.

## Change

Commit `581da609` introduced shared camera results for terrain demand and
presentation. Its cache could return before sampling time-dependent collision
recovery when streaming/UI work advanced the client clock. Build 160 checks the
sampling tick before that immediate return. On a later tick it advances recovery
first, then reuses the spatial collision result only if the resulting pose and
all providers still match. An unchanged camera retains its expensive query result.

See the [camera-sharing review](cpu-cutover-gap-review.md#earlier-camera-sharing-missing-clock-dependency)
for the source history, stock `603D30` dependency and remaining diagnostic limits.

## Validation and scope

- Formatting and workspace Clippy, all targets/features with `-D warnings`, pass.
- Full workspace tests pass: 1,600 passed, zero failed, 33 existing ignored tests.
- New regressions cover recovery advancing between demand and presentation,
  clock wrap/provider invalidation, and spatial reuse after an unchanged later
  sample. They exercise the real Systems height recovery and camera projection.
- Numbered `test-client` compilation and `-SkipBuild` installation succeeded;
  installed revision, build number, executable hash and Testing shortcut match.

This is a focused camera correction, not a completed CPU cutover or a measured
FPS improvement. No interactive client was launched. The reported blur/choppiness
remains open until an equivalent moving Soap view establishes whether this
correction addresses it; numerical parity does not establish displayed smoothness.
