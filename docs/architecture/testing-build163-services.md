# Testing Build 163: contextual services and scheduler publication

Build 163 packages source `1b9bf3fb8504ace34a251ac920b9fa5cf76af156` from
`perf/critical-frame-work`. Installed through the Testing shortcut on 2026-09-20
at 07:32 EDT. Installed and packaged executable SHA-256:

`34C5E6C92CA37CFE8C383E0C6DA2678E3B9443935BB600731202DCC5D77E9DE0`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed before compilation; tracked files and the index
remained frozen until the optimized build completed.

## Included changes

- Reserved frame runners publish under one dispatcher lock and notification.
  Sampled lock, queue and useful-wake intervals distinguish scheduler boundaries.
  See [the scheduler measurements](cpu-dispatch-publication.md), including the
  observed empty-kernel contention and overhead limits.
- Finite and resumable services receive admitted `JobContext` control, diagnostics,
  scoped scratch and cooperative withdrawal. Dropping a consumer still leaves
  executor ownership and required cleanup intact.
- Obsolete terrain demand withdraws between whole preparation operations and
  returns its mounted bank without a partial resident. Current demand preserves
  its existing error/publication behavior. Task control/completion and terrain
  preparation have focused folder modules.

See [the service adoption record](cpu-service-context.md). These changes preserve
the camera correction and existing worker-policy defaults.

## Validation and remaining cost

- Formatting and full workspace Clippy, all targets/features with warnings denied,
  pass. Full workspace tests: 1,617 passed, zero failed, 33 existing ignored.
- Optimized numbered compilation and `-SkipBuild` installation succeeded. Build
  identity, executable hash and Testing shortcut were verified. Launch settings
  retain four CPU workers, capacity 256, two network workers, 2560 x 1440
  fullscreen-windowed and GPU 0. No interactive client was launched.
- The isolated optimized population fixture completed 20,480 frames. All five
  captures report zero dropped samples/events/trace rows and capacity overflows.
  All sampled root palettes were consumed. The fixture omits live networking,
  movement solving, audio and overlays; it is not live FPS validation.

The [population study](npc-density-cutover-measurements.md) reproduces about
4.5 ms of added frame cost at 192 authored NPCs, spread across model preparation,
publication and rendering. **This package does not fix that scaling problem or
claim a measured FPS gain.** It advances the CPU cutover; typed cross-domain
products, remaining working-set accounting and domain adoption remain required.
