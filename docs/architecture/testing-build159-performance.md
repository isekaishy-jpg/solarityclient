# Build 159 live F10 review

The 2026-09-20 capture `1789889511517-1` records Testing Build 159, source
`8a4ae6813ff09df27202540a1737f4541c3c32cc`, for 110.091 seconds. The matching log
is `solarity-20260920-033146.log`: four CPU workers, two network workers,
2560 x 1440 and a GeForce GTX 1070. All dropped-row/sample and capacity-overflow
counters are zero. The subsequent 03:34 launch was still active during this
read-only review; it was not interrupted or used as completed-run evidence.

## Separate world play from loading and shutdown

The capture includes Glue, loading and quitting. The overall ordinary-frame
average after two seconds is 5.588 ms (179 FPS), but this is not an in-world FPS
result. The world-scene samples begin around 23 seconds. This review selects
writer intervals ending after 25 and at/before 109 seconds, excluding loading
and the final 324.723 ms session-action/quit transition.

That window contains 9,926 ordinary frames: mean **8.454 ms (118.3 FPS)**,
median 8.065 ms, p95 10.831 ms, p99 19.098 ms and maximum **32.304 ms**.
Its 78 sampled detail frames average 10.216 ms; detail instrumentation adds
work and must not be pooled with ordinary frames.

This is not a matched replay of Build 155. Its earlier 9.059 ms ordinary mean
does not establish a controlled improvement here. The later part of this run
gets faster: ten-second windows at 30-80 seconds average about 9.0-9.5 ms,
then the 90-100 and 100-109 second windows average 7.091 and 6.968 ms. Changes
in scene/view remain confounding factors; this run does not show a steady
frame-rate decline over time.

## Main-thread work remains the limiting CPU path

OS CPU deltas within the selected world window show main using **97.46% of one
logical CPU**. Protected workers use 9.34%, 9.87% and 9.78%; the flexible worker
uses 11.50%. These percentages refer to one logical CPU, not the whole machine.
An unnamed native thread uses 13.53%; the capture does not establish its owner.

| Inclusive main scope | Mean per ordinary world frame |
| --- | ---: |
| All instrumented M2 CPU preparation | 3.665 ms |
| M2 admission/resumption spans | 2.484 ms |
| M2 publication spans | 1.185 ms |
| World/session service | 1.426 ms |
| Vulkan presentation | 1.749 ms |
| Vulkan command recording, within presentation | 1.028 ms |
| World FrameXML update/upload | 0.587 ms |

These scopes overlap; do not add parent/child, worker or GPU measurements.
The GPU's 78 world samples average 3.317 ms, including 1.333 ms for shadows.

The coarse `World scene preparation / M2 admission` phase is now only 0.991 ms,
but it no longer contains all admission work. The world driver calls
`pending_m2.try_advance` during later main continuations. Its phase named
`independent ground detail and WMO packets` averages 1.299 ms and the later
`M2 and lit surface publication` phase averages 1.566 ms. These labels must not
be interpreted as isolated subsystem costs. The older Build 155 M2 CPU aggregate
was 3.982 ms; this run's 3.665 ms aggregate demonstrates that the much larger
drop in the first phase label is largely redistribution, not equivalent work
elimination. Different scenes prevent assigning the remaining difference to
the code changes.

Grouped work and static admission are executing on workers. Ordinary executor
groups average roughly 27-29 microseconds per call; static-admission groups
average 11-12 microseconds. CPU result consumption averages 0.492 ms per world
frame and includes the consumer; it is not a native blocked-time measurement.
The main pending-result scope averages 0.028 ms per frame and includes useful
servicing. No CPU failure or capacity-overflow report was found.

## Metadata retention works; other residency scans remain

Of 379 ordinary world topology publications, **290** retained static indices and
**89** used full publication. Partial metadata publication averages **0.143 ms**
per invocation, maximum 0.285 ms. Full metadata publication averages 1.833 ms,
maximum 4.324 ms. The outer topology operation peaks at 4.341 ms.
Only 18 publications rebuilt the static spatial index: full metadata publication
must not be equated with rebuilding that index.

The selected scene averages 24,555 resident placements, including about 430
dynamic placements. The prior optimized fixture had only 28 dynamic placements;
its five-to-seven-microsecond result was not a live-scene prediction.

The largest ordinary world frame, **14845**, takes **32.304 ms** and contains two
sequential expensive phases:

- World/session service: 14.110 ms, including **9.499 ms creature/remote-player
  residency** and 3.475 ms terrain service.
- World preparation: 17.047 ms, including **8.917 ms unit-state updates**.

The same pair appears on nearby frames 14814 and 14023. The code contains a
concrete structural problem consistent with these correlated spikes:

- [`replace_creatures`](../../crates/runtime/src/application/terrain_frame/m2/mod.rs)
  and `replace_remote_players` each use `placements.iter().any(...)` separately
  for every input to find retained owners/generations. They traverse the full
  placement storage, including scenery.
- `dynamic_placement_index` in the same file switches from the retained owner
  lookup to `placements.iter().position(...)` whenever topology is dirty.
  `update_player_state`, `update_creature_states` and `update_remote_player_states`
  call it before later M2 preparation publishes topology. Therefore a residency
  mutation can make the following unit-update pass repeatedly scan scenery too.

This is repeated unit-count times resident-count lookup work, and it survives
Build 159's metadata-publication change. The coarse phases include other work;
the capture does not prove that these scans account for every millisecond of
either phase. The direct next correction is current owner/generation lookup
independent of deferred render-metadata publication, preserving native first-owner
semantics through insertion, removal, retirement and ordering. This review does
not implement that correction.

## Loading, resources and limits

The log's accepted-world to scene-ready milestones span approximately 5.224
seconds. During loading, frame 8669 takes 131.420 ms: **131.059 ms in one UI
startup slice**, including event dispatch and a 101.170 ms event handler.
Frame 8691 includes **67.971 ms scene GPU publication**. Startup slicing has
not bounded every indivisible callback/publication step.

GPU frame-slot growth is also logged during world play. Ordinary buffer growth
must not automatically be called a device-idle drain: the source's explicit
`device_wait_idle` branch is for slot-count/extent changes; normal slot growth
uses the current slot's lifecycle. The log alone does not isolate the time
attributable to each growth operation.

Working set in the selected window goes from 2,056.8 MiB to a 2,280.2 MiB peak
and ends at 2,187.2 MiB. Private bytes go from 3,110.0 MiB to 3,357.8 MiB at that
peak, then 3,283.7 MiB at the end. Retention is substantial, but this rise and
partial fall does not establish a continuous leak. Fourteen warnings concern
the existing optional Tauren texture replacements. No logged error or panic
appears, and the run ends with `UiQuitRequested`.

Raw files remain under `%LOCALAPPDATA%/SolarityClient/testing/Profiles/`.
Trace SHA-256: `E5B30A91F8B22F120468BF09CD05CD15A0E7CEBDDE1B3B0514B931325F1ACB7A`.
Ignored `target/build159-world-audit.py` produces the selected-window JSON/report;
`target/build159-live-audit.py` and `scripts/analyze-profile.py` cover the complete
capture. Median and slow sampled-frame reports are
`target/build159-live-{median,slowest}.{txt,json}`. Ordinary spike 14845 has only
seven unsampled slow spans, so its coarse phase timings come from events rather
than a complete nested trace. Production source and the installed build are
unchanged by this review.
