# Testing Build 169: retained owner attachment inputs

Build 169 packages source `75a21fcab862ce6c7e9bb7f52441612c09081ae5` from
`perf/critical-frame-work`. Installed through the Testing shortcut on
2026-09-20 at 15:55 EDT. Installed and packaged executable SHA-256:

`4438FA28C858ADA857B4E263B939CDD24A444677FA3CCB1A6DABE3A49E65EE66`

The package reports dirty because reserving `BUILD_NUMBER` modifies that tracked
file. Source was committed and the checkout clean before compilation; tracked
source and index remained frozen throughout the numbered build.

## Included change and validation

Topology-owned attachment groups and ordered frame-sample indexes replace
repeated scene-wide lookups in hand posing, bone demand, attachment publication
and parent admission. Duplicate requests, first-parent selection, any-hidden-rider
handling, missing-parent errors and stock camera/callback order are preserved.
The module is decomposed into request and sample storage. An installed-item
inspection example establishes explicit gear definitions for equipped fixtures.
See the [contract and full measurements](m2-attachment-inputs.md).

- Formatting, all-target/all-feature workspace Clippy with warnings denied and
  all 1,638 tests pass; zero failed, 33 existing ignored, 101 suites. This includes
  four new index cases and existing equipment, mounts, retirement/GUID reuse and
  moving M2 geometry parity tests.
- Four alternating 300-equipped-NPC runs complete 16,384 frames. Matched
  stationary medians improve by 0.232-0.414 ms; tails are mixed, and the second
  candidate matched p95 increases. Hitches remain.
- An unarmed 192-NPC control pair completes 8,192 frames with essentially flat
  stationary medians and mixed tails. A separate equipped profiled pair completes
  another 8,192 frames: main admission falls by approximately 0.421 ms/frame,
  while publication remains effectively unchanged.
- The equipped profile still measures main M2 admission at 4.099 ms/frame,
  publication at 3.001 ms/frame and renderer CPU time at 4.655 ms/frame.
  Geometry assembly and transparency ordering are significant parts of the
  remaining publication cost. Nested scopes and GPU time must not be added
  indiscriminately. These hidden offline fixtures do not establish live FPS.
- Numbered optimized compilation and `-SkipBuild` installation succeed. Revision,
  build number, executable hash and Testing shortcut are verified. Settings
  preserve four CPU workers, capacity 256, two network workers, 2560 x 1440
  fullscreen-windowed and GPU 0. No interactive client was launched.

This is a measured scaling checkpoint, not the requested multi-ms fix or completed
CPU cutover. The [remaining requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active, including main-thread distribution and domain memory accounting.
