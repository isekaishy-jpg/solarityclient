# Live Soap shadow preparation follow-up — 2026-09-13

The final candidate averaged **156.0 and 158.8 FPS**, bracketing an unchanged
Build 125 diagnostic baseline at **119.5 FPS**. This is an observed 31–33% gain
in this stationary live view. Model preparation fell from 4.242 ms to
2.624 / 2.587 ms, and command recording from 1.143 ms to 0.971 / 0.956 ms.
The large model traversal reduction is consistent with the changed hot path.
Live population and world-service costs also varied, so the entire FPS change
cannot be assigned precisely to one optimization.

This follows [the Build 125 live comparison](testing-build125-world-entry.md).
It does not establish 300–400 FPS in the real client. The earlier synthetic
test used a different view, shadow quality and workload.

## Code changes

- `terrain_frame/m2/mod.rs`: reject static scenery from compact cached distance
  metadata before touching large placement/source records for environment
  shadows. The existing size-class shadow cutoff is reused exactly. Root and
  attachment admission reuse precedes this shortcut; uncached/moving objects
  retain the original path. No shadow quality, distance, visible culling or
  animation policy was reduced.
- `terrain_frame/m2/shadow.rs`: retain material samples from shadow packet
  preparation for reuse by the same placement's ordinary draws. Scratch is
  cleared at every placement boundary, so clocks and instances cannot share
  stale material samples.
- `vulkan_world_frame/command/shadow/`: use existing command binding state to
  avoid repeated pipeline/vertex/index binds, resetting state at each shadow
  pass. Descriptor sets and draw order remain as before.
- Test-only live diagnostics report retained model workload at renderer capture.
  Regression coverage now also includes cached far props inside a cascade but
  beyond the existing size-class cutoff, alongside offscreen near props,
  animated sources, attachments, and the inverted-bounds case.

The final captures reported 22,777 / 22,917 retained placements, only 369
visible draws, and 14 primary shadow draws. Environment shadow packets were
773 / 796; dynamic placement counts were 49 / 189. The static resident
population was therefore 22,728 in both captures. Traversing resident props
remains real CPU work even when they contribute no visible draw. Counts are
capture-time snapshots, not averages across the measurement interval.

## Final comparison

| Metric | Candidate 1 | Baseline | Candidate 2 |
| --- | ---: | ---: | ---: |
| FPS | 155.98 | 119.48 | 158.78 |
| Frame time, ms | 6.411 | 8.370 | 6.298 |
| Model preparation, ms | 2.624 | 4.242 | 2.587 |
| Instance traversal (inside model preparation), ms | 2.348 | 3.909 | 2.315 |
| Command recording, ms | 0.971 | 1.143 | 0.956 |
| Queue-present host call, ms | 0.125 | 0.122 | 0.123 |
| Session/world service, ms | 0.847 | 0.991 | 0.832 |
| Worst observed frame, ms | 39.441 | 41.879 | 43.360 |

Parent and child scopes overlap; do not sum these rows. Long-frame spikes
remain, and this work does not demonstrate improved worst-case latency.

All three use Soap at the Orgrimmar gate, GTX 1070, 2560x1440
fullscreen-windowed, VSync off, shadow quality 5, farclip 1277,
environmentDetail 1.5, and 49 resident terrain tiles. They run the normal client
loop with the same installed Testing profile, network/world services, UI,
audio and FPS overlay. CPU timings are enabled; GPU timestamps are disabled.
Stock WoW and the local servers remain running throughout. Realm time and
population are live rather than replayed.

Each run settles for 20 seconds, captures through the renderer, then emits a
measurement marker. Analysis selects the 30 two-second reports ending 10–70
seconds after that marker: about 60 seconds each, weighted by frame count.
No builds or tests ran during these intervals. Captures were converted after
the final measurement. Visual review confirms the same camera, terrain,
buildings and props, with expected live animation/population variation; this
is not a pixel-identical replay or proof for every scene.

## All exploratory runs and presentation confound

Run labels below map to `target/shadow-<label>` in the canonical checkout.
Every row uses the same report-window selection.

| Run | Revision | GPU timings | FPS | Frame ms | Model ms | Record ms | Queue present ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| `before-one` | Baseline | on | 126.51 | 7.905 | 3.920 | 1.129 | 0.121 |
| `before-cpu-only` | Baseline | off | 87.50 | 11.429 | 3.843 | 1.112 | 3.740 |
| `after-cpu-one` | Bindings/material reuse | off | 127.70 | 7.831 | 3.980 | 0.973 | 0.124 |
| `before-cpu-two` | Baseline | off | 118.59 | 8.432 | 4.309 | 1.124 | 0.124 |
| `after-cpu-two` | Bindings/material reuse | off | 127.94 | 7.816 | 4.004 | 0.964 | 0.122 |
| `before-cpu-three` | Baseline | on | 87.35 | 11.448 | 3.862 | 1.125 | 3.748 |
| `before-cpu-four` | Baseline | off | 87.14 | 11.476 | 3.852 | 1.135 | 3.802 |
| `after-prefilter-one` | Final candidate | off | 155.98 | 6.411 | 2.624 | 0.971 | 0.125 |
| `before-cpu-five` | Baseline | off | 119.48 | 8.370 | 4.242 | 1.143 | 0.122 |
| `after-prefilter-two` | Final candidate | off | 158.78 | 6.298 | 2.587 | 0.956 | 0.123 |

The unchanged baseline exhibited two presentation states: roughly 3.7 ms
inside queue-present and roughly 0.12 ms. Both occurred with GPU timestamps
enabled, and both occurred with them disabled. An initial hypothesis that
GPU queries caused the state change was therefore disproved as a sufficient
explanation. Foreground/compositor/driver behavior is a possibility, not a
verified cause. The 87-to-120+ FPS jump across these states must not be credited
to the patch. The final bracketing comparison uses the low-wait state throughout.
The misleading `before-cpu-three` label actually has GPU timestamps enabled;
it is retained as diagnostic evidence and excluded from CPU-only comparisons.

The initial GPU-timed baseline sampled 467 frames, averaging 3.300 ms from
its first to last GPU marker: shadows 0.753 ms, uploads/sky 0.521 ms, terrain
1.161 ms, WMO 0.123 ms, ground detail 0.032 ms, models/effects 0.216 ms,
screen effects 0.442 ms and UI 0.049 ms. These are sampled elapsed GPU
intervals, not isolated shader costs. They do not support attributing the
entire synthetic/live gap to shadows or networking. Final candidate GPU cost
was not measured in this CPU-only comparison.

## Validation and executable identity

The bindings/material pass passed the complete workspace all-feature suite
(1,353 tests passed, 26 intentionally ignored). After adding the compact
prefilter and expanding its regression, the final workspace library suite
passed (454 tests, 23 intentionally ignored); workspace Clippy with all
targets/features and warnings denied, plus formatting checks, also passed.
The final optimized `test-client` diagnostic build completed successfully.
The full integration suite was not repeated after the final prefilter edit.

All candidates derive from branch parent `5f4abf39`. The baseline is the sealed
Build 125 diagnostic executable described in the preceding report. All use
the same optimized `test-client` profile. Candidate metadata still says Build
125 because these are development diagnostic executables, not new installer
packages. Build 126 has not been reserved or installed. Soap remains running
in the final candidate after measurement.

SHA-256:

- `live-build125.exe`: `64AA57B5D8B54539051F15E1E423D1BD46EB7F431511D7A1133222BF95D637E3`.
- `live-shadow-candidate.exe`: `61D5B34FCA82C1D24055D9647CDE5C42031B073F279E2CC0777C959C06584A2A`.
- `live-shadow-prefilter.exe`: `64FC45F413B243F81663ABD532CB716D5149384603BFE664E434E8796AEFF4E7`.

Local evidence is retained under the canonical checkout's `target/`:

- Each run's `.log`, `.err.log`, `-summary.json`, `.scopes.json` and
  `.vulkan.json`; the GPU run additionally has `.gpu.json`.
- `shadow-before-cpu-five-scene.png`, `shadow-after-prefilter-one-scene.png`
  and `shadow-after-prefilter-two-scene.png`, with their original PPM captures.
- `shadow-work-checks.log`, `shadow-prefilter-checks.log`,
  `shadow-candidate-build.log`, `shadow-prefilter-build.log`,
  `shadow-evaluation.log`, and `shadow-prefilter-evaluation.log`.
- `analyze-live-fps.py`, `analyze-live-scopes.py`, and
  `start-shadow-comparison.ps1` document analysis and launch settings.
- `shadow-final-source.json` records the exact candidate source hashes.
