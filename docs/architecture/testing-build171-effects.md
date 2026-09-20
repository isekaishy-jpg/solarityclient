# Testing Build 171: worker-produced effect upload

Build 171 packages source `dae47563c69c5e5ba4d1e98108aebad63e55c35e` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 17:35 EDT. Installed and packaged executable SHA-256:

`33057C66CFC2903C1B88640DCB860589A081269368F1B1231223AEF40DCAEC1C`

Dirty identity reflects the reserved tracked `BUILD_NUMBER`. Source was committed
and the checkout clean before compilation. Tracked source and index remained
frozen throughout the numbered build.

## Change and validation

Particle, ribbon and index streams upload directly from the workers' final
representation. Fixed POD vertex layouts remove per-element serialization on
main without new allocations, staging copies, tasks or joins. Camera sampling,
stock animation ordering, effect RNG and slot fences retain their behavior.
See the [contract and measurements](effect-stream-upload.md).

- Formatting, workspace Clippy with all targets/features and warnings denied,
  and all 1,646 tests pass; zero failed, 33 existing ignored, 101 suites.
- Eight hidden controlled runs complete 16,384 frames before packaging, without
  compiler or GPU-test overlap. Two equipped pairs show 3.118-5.518 ms lower
  matched stationary medians; the unarmed control improves 0.319 ms. Tails are
  mixed, including a worse first-candidate streaming maximum.
- Separate profiles reduce slot wait/upload by 0.591 ms and renderer thread
  cycles by about 11.1%, but total profiled frames worsen from 23.689 to
  25.805 ms. Large native presentation waits and higher main admission remain.
  The unprofiled gains cannot all be attributed to CPU encoding or generalized
  to live performance.
- Numbered optimized compilation and `-SkipBuild` installation pass. Revision,
  build number, hash and Testing shortcut are verified. Settings preserve four
  CPU workers, capacity 256, two network workers, 2560 x 1440 fullscreen-windowed
  and GPU 0. No interactive client was launched or interrupted.

The complete CPU cutover remains open. Main admission, remaining domain
integration and movement/loading scaling are not completed by this package.
