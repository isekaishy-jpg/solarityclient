# Testing Build 125: world-entry readiness

Build 125 contains the world-entry terrain/detail readiness fix described in
the Build 124 follow-up. Soap is the default live test character.

The loading card now waits for GPU publication of every required terrain tile
and completion of visible ground-detail jobs. A completed card cannot close
after its readiness becomes invalid. Map acknowledgement retains its original
prerequisites; player completion waits for the scene. Ordinary gameplay keeps
its existing frustum calculation and incremental tile admission.

## Validation

- Final workspace Clippy, all targets/features, warnings denied: passed.
- Final workspace tests: 1,353 passed, zero failed, 25 ignored.
- Formatting: passed.
- Separate installed-data regression: passed. The card remained until all 49
  required tiles and visible detail were ready, then rejected regressed
  readiness and a retired map. Its unoptimized readiness loop took 22.287s;
  this is not a packaged-client loading-time or FPS measurement.
- Optimized test-client build, installation, and launch: completed.

## Artifact

Build 125 reports revision `3a965accbd643af08d985d10db3d19f67b74f949`, dirty=true,
because the subsequent source fix and BUILD_NUMBER were packaged before their
commit. Installed SHA-256:
`645DDEAA86E5C9F70A1B24DBAB613ECFB7FFCA82D4E07D13D84B982814B8A9E0`.

The Testing launcher enables SOLARITY_FRAME_TIMINGS and preserves the existing
profile. Installed Build 124 is backed up at the canonical checkout's
`target/solarity-runtime-build124-installed.exe`, with its build-info file.

The subsequent live comparison below measures Soap through matching diagnostic
launchers. The prior offline fixture's 400+ FPS does not represent this scene,
and the user's roughly 120 FPS observation was not reproduced in this fixed
view. Do not extrapolate the measured percentage to that earlier observation.

Evidence in the canonical checkout: `target/world-entry-target-test.log`,
`target/world-entry-final-checks.log`, `target/build125-world-entry.log`, and
`target/build125-install.log`. Per-session timing logs are in the installed
Testing client's `logs` directory.

## Live Soap comparison

The user authorized login to the local server and named Soap as the default
test character. Windows desktop capture failed, so the ignored live diagnostic
uses the existing native Lua login and character-selection calls. Credentials
come from the explicitly supplied process environment; the source contains no
account or password values. It selects only the uniquely matching character.
After entry and a 20-second settling interval, it captures the rendered scene
and hands off to the unchanged `ClientApplication::run` game loop.

Matched stationary runs show **5.31% higher FPS**, with mean frame time
reduced by 5.04%:

| Live diagnostic | Frames | Represented seconds | FPS | Mean frame ms | Worst observed frame ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Build 123 | 5,204 | 60.144 | 86.53 | 11.557 | 25.517 |
| Build 125 | 5,482 | 60.161 | 91.12 | 10.974 | 20.207 |

Build 123's two-second FPS windows range from
83.21 to 88.02;
Build 125's range from 90.27 to
92.26. An earlier settled Build 125 sample was
91.86 FPS; it is supplementary and is not pooled into the paired
result. Outliers remain. Aggregated reports do not provide full raw-frame
percentiles, and the maximum column does not establish a general stutter fix.

The main-loop presentation scope averages
10.649 -> 10.190 ms;
session/world service averages
0.897 -> 0.775 ms.
These are wall-clock application scopes and do not independently isolate GPU
execution or each individual optimization.

### Conditions and limitations

- Soap, level 80, stationary in Durotar outside the Orgrimmar gate. Rendered
  captures show the same camera and scene. Saved distance 15 and pitch 7.652950.
- NVIDIA GeForce GTX 1070, 2560x1440 fullscreen-windowed, VSync off.
- The same installed Testing profile: shadow quality 5, farclip 1277,
  environmentDetail 1.5. Both captures report 49 resident terrain tiles.
- Real local login/world servers, character data, network traffic, world UI,
  audio, movement services, remote population, and FPS overlay remain active.
- No compiler runs during the sampled intervals. The existing stock WoW
  process remains present throughout both samples; other desktop/server load
  is not isolated. Live realm time and population can vary between sessions.
- Both revisions are compiled with `test-client`. These are matching diagnostic
  test executables that invoke the normal game loop, rather than direct captures
  of the two previously installed package executables. The startup adapter is
  test-only and does not alter production login behavior.
- Each result uses 30 complete two-second `live client frame` reports whose end
  timestamps fall 10-70 seconds after the measurement marker. FPS is total
  sampled frames divided by summed represented frame time. Screenshot capture
  precedes that marker. This covers one stationary view, not combat or travel.

Baseline measurement marker: `2026-09-13T03:24:04.109182Z`.
Candidate measurement marker: `2026-09-13T03:28:12.581066Z`.

### Revisions and reproduction evidence

Baseline source: main `f5f5f4a81e5c11241f4c80c117e5dfe9b587dcee` (Build 123).
Candidate source: `4992e473` (Build 125 loading fix). The same two ignored
diagnostic modules are attached to each revision. The baseline's original
source bytes were restored immediately after compilation, verified against
saved hashes. The candidate retains the diagnostic source for future explicit
live testing; it requires credential environment variables and is not a
completed automated unit-test run. Soap is left in the candidate's live loop.

Diagnostic executable SHA-256:

- Build 123: `3EE8C4788C0F51D93B16217DB76B316B610215433A77C739154A21C6BC85C2DD`.
- Build 125: `64AA57B5D8B54539051F15E1E423D1BD46EB7F431511D7A1133222BF95D637E3`.

Evidence under the canonical checkout:

- `target/live-build123-one.log`, `target/live-build125-two.log` and their
  `*-summary.json` files retain the paired data.
- `target/live-build123-one-scene.png` and `target/live-build125-two-scene.png`
  retain the renderer captures; original PPM files are also retained.
- `target/live-build125.log` and `target/live-build125-summary.json` retain the
  preliminary candidate run; later compiler activity in that log is excluded.
- `target/analyze-live-fps.py` reproduces the window selection and weighted FPS.
- `target/live-session-build.log`, `target/live-baseline-build.log`, and
  `target/live-diagnostic-lint.log` record diagnostic compilation and linting.
- `target/live-build123.exe` and `target/live-build125.exe` are the sealed
  diagnostic binaries. `target/solarity-runtime-build125-installed.exe` keeps
  the installed package separately from these diagnostic executables.

## Remaining performance gap

The paired candidate is about 24% below the user's roughly 120 FPS reference,
but the baseline also runs below that reference in this session. The evidence
therefore supports a modest branch gain; it does not establish a regression
from the earlier 120 FPS observation or explain the difference in conditions.

Weighted application scopes from the same measured windows:

| Scope / phase | Build 123 ms | Build 125 ms |
| --- | ---: | ---: |
| Model (M2) packet preparation | 3.908 | 3.611 |
| Instance traversal within model preparation | 3.492 | 3.230 |
| Vulkan presentation, including CPU work and waits | 5.359 | 5.342 |
| World camera during presentation | 0.148 | 0.002 |
| World UI update and upload | 0.603 | 0.608 |

These scopes nest; do not sum parent and child rows. Vulkan's more detailed
candidate timings include 1.104 ms command recording and 3.741 ms in the
queue-present call (baseline 1.123 and 3.726 ms). Fence wait is only 0.006 ms.
Queue-present is host wall time inside the driver call, not a GPU timestamp;
it cannot distinguish driver/compositor waiting from GPU backpressure. The
largely unchanged presentation cost and remaining model traversal are the
largest measured areas for further investigation. Neither identifies the
cause of the difference from an unmatched earlier observation.

A separate candidate launch removes SOLARITY_FRAME_TIMINGS entirely and uses
the same executable, profile, character, view, and 20-second settling period.
Its renderer capture displays **92.9 FPS**, with all 49 tiles resident. This
spot check suggests profiling is not sufficient to explain the shortfall to
120 FPS; it is not an averaged unprofiled benchmark or an exact measurement of
profiling overhead. Soap remains in this unprofiled candidate session.

Additional evidence in the canonical checkout:

- `target/live-build125-unprofiled.log` and
  `target/live-build125-unprofiled-scene.png` (capture at
  `2026-09-13T03:47:56.497703Z`). No frame-profile reports appear in this log.
- `target/analyze-live-scopes.py`, paired `*.scopes.json`, and paired
  `*.vulkan.json` reproduce the nested timing breakdown above.

## Subsequent shadow preparation work

The [2026-09-13 follow-up](live-shadow-work-2026-09-13.md) documents the
presentation-state confound and the later repeated live gains from reducing
model shadow preparation and redundant command bindings.
