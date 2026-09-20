# Testing Build 172: contextual CPU execution

Build 172 packages source `187a792bdd6c42546de2be9c074f8d2775dc4c4b` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 18:55 EDT. Installed and packaged executable SHA-256:

`21A21EF02179CEB75FDF903295BA137D8E29F01A99032B70BF09AFC71AE8678B`

Dirty identity reflects the reserved tracked `BUILD_NUMBER`. Source was committed
and the checkout clean before compilation. Tracked source and index remained
frozen throughout the numbered build.

## Connected changes

Every production CPU submission now receives `JobContext`. Finite services and
bulk loading have separate execution eligibility, loading graphs can use multiple
flexible workers, and runtime defaults divide protected and flexible capacity
with the configured worker count. Numeric state is restored at service/kernel
boundaries, including discarded-result cleanup. Skeletal palettes own byte
reservations; minimap archive/texture loading yields between stages and preserves
its archive owner when admission is full. Shadow recording supplies calibrated
cost hints. See [the implementation report](cpu-service-execution-cutover.md).

## Validation

- Formatting and workspace Clippy with all targets/features and warnings denied
  pass. All 1,654 tests pass, zero fail, and 33 existing tests remain ignored.
- One hidden baseline/candidate pair completes 4,096 frames with 300 equipped
  NPCs. Steady-state results are modest and mixed; the candidate has a 344 ms
  first-frame spike. This is not evidence of a large live FPS improvement.
- A separate 896-frame candidate travel replay completes without a crash or
  storage-admission failure. Its first frame is 98 ms. These hidden fixtures do
  not reproduce live networking, movement solving, sound or desktop presentation.
- Numbered optimized compilation and `-SkipBuild` installation pass. Revision,
  build number, hash and Testing shortcut are verified. Settings retain four CPU
  workers, capacity 256, two network workers, 2560 x 1440 fullscreen-windowed,
  and GPU 0. No interactive client was launched or interrupted.

The full CPU cutover remains open for the requirements in the
[status document](cpu-cutover-status.md). This package does not complete main
admission, all resource dependencies, GPU waits or whole-process qualification.
