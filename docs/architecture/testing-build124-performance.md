# Testing Build 124 performance evaluation

Testing Build 124 packages `perf/critical-frame-work` revision `a7296a7debbb29c20d4792ec75bcb8104ea1fb93`, version 0.0.3a. The optimized client and benchmark compile successfully. The Testing installer was run with `-SkipBuild`, preserving the single reservation of number 124. The runtime reports `dirty=true` because packaging changed `BUILD_NUMBER`. No application source was changed during this evaluation.

## Results

The longer offline comparison improves aggregate throughput from 409.6 to 434.3 FPS (+6.03%). Fully resident stationary/orbit/pointer phases improve by 5.4-8.5%. Two shorter pairs independently reproduce a 5.02% gain in the fully resident pointer phase. These are measurements on this fixture, not populated live-server FPS claims.

| Phase, 8,000 frames each | Build 123 FPS | Build 124 FPS | FPS change | Before p99 ms | After p99 ms | Before max ms | After max ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| stationary | 442.0 | 479.4 | +8.48% | 3.385 | 2.463 | 6.127 | 3.334 |
| orbit | 401.8 | 423.5 | +5.40% | 2.961 | 2.833 | 3.624 | 3.277 |
| pointer | 385.7 | 406.6 | +5.42% | 3.341 | 3.023 | 8.820 | 6.936 |
| travel_out | 397.0 | 415.9 | +4.76% | 3.047 | 3.045 | 17.545 | 13.092 |
| travel_back | 394.7 | 416.3 | +5.49% | 3.040 | 3.024 | 13.763 | 16.557 |
| settled | 444.3 | 475.7 | +7.07% | 2.626 | 2.468 | 3.315 | 9.771 |
| all_non_loading | 409.6 | 434.3 | +6.03% | 3.039 | 2.874 | 17.545 | 16.557 |

Across the long non-loading phases, mean camera work falls from 0.11606 to 0.01384 ms (about 88%); mean UI work falls from 0.31092 to 0.29812 ms (about 4%). Mean streaming work rises from 0.16019 to 0.17636 ms. These host scopes support camera-query savings as the principal measured contribution; they do not isolate every branch change or GPU cost.

The long run has three baseline frames above 16.67 ms versus zero candidate frames, but the two short candidate runs still reach 32.967 ms and contain five frames above 16.67 ms versus four in the baselines. The short-run candidate p99 rises from 3.101 to 5.262 ms as deferred work crosses phase boundaries. The branch has not eliminated stalls.

## Loading and comparability

The raw short-run aggregate rises from 403.3 to 441.8 FPS (about +9.6%), but that is confounded by deferred loading and must not be presented as an equal-work overall gain. Baseline initial streaming finishes within the first phase; the candidate reaches all 49 tiles only late in orbit. The candidate returns with 35 resident tiles in those short runs and completes the remaining admissions during the nominal settled phase.

| Replay | First 49 resident tiles: phase/frame | Measured elapsed seconds |
| --- | --- | ---: |
| before-one | streaming/1648 | 3.797 |
| after-one | orbit/2057 | 12.530 |
| before-two | streaming/1242 | 2.657 |
| after-two | orbit/2130 | 12.403 |
| before-long | streaming/1227 | 2.659 |
| after-long | streaming/7018 | 11.824 |

These elapsed times sum replay frame intervals after the benchmark begins, excluding bootstrap/loading-screen setup. They measure neighbor residency completion, not login or total client startup.

In the long pair both builds start stationary, orbit, and pointer phases with 49 tiles, and have no admissions in those phases. Row-by-row checks compare residency, fixture coordinates, ground-detail/shadow draw counts, environment-shadow packets, liquid/effect identities, and effect parameters. See the retained residency JSON for exact mismatch counts. Streaming is asynchronous, so travel frame membership need not match. Build 124 finishes the long return with 47 tiles, then admits two in settled; the full settled mean is not a strictly identical residency interval.

Ground-detail draw counts differ on 35 of 8,000 orbit frames; all other checked orbit fields match. Settled residency differs on 66 frames. Restricting both builds to matching phase/frame rows for every checked field gives the following results. This checks recorded workload counters, not pixel-perfect visual parity.

| Matched phase | Matched frames | Build 123 FPS | Build 124 FPS | FPS change |
| --- | ---: | ---: | ---: | ---: |
| stationary | 8000 | 442.0 | 479.4 | +8.48% |
| orbit | 7965 | 401.9 | 423.5 | +5.39% |
| pointer | 8000 | 385.7 | 406.6 | +5.42% |
| settled | 7934 | 444.4 | 476.1 | +7.15% |

## Method and validation

NVIDIA GeForce GTX 1070, 2560x1440 windowed, VSync and maxFPS disabled, shadow quality 2, identical copied isolated profiles, installed enUS data. Map 1, start `(1100, -4500, 150)`, travel offset `(-1600, 0, 0)`, camera distance 25, noon realm clock. CPU workers 4, queue capacity 256. CPU/GPU diagnostics and capture disabled. No compiler or other Solarity runtime ran during measurement.

The unchanged production `benchmark_world` example was freshly built from main `f5f5f4a81e5c11241f4c80c117e5dfe9b587dcee` for the Build 123 baseline and from the branch for Build 124, both with the `test-client` profile. Order: 123/124/123/124 at 2,400 frames per phase, then 123/124 at 8,000 frames per phase. Six phases are measured separately from initial streaming. All six replays completed successfully, with no warnings/errors in their log files. The fixture has no authored NPCs, live network, movement solver, audio, or overlays, so it does not validate the new asynchronous remote-population/network behavior under playerbot load.

The previously recorded branch validation includes formatting, warning-free workspace Clippy, 1,352 passing workspace tests (24 ignored), and the additional real worker completion regression. No source edits followed it in this packaging task, and the client/benchmark builds and replay runs provide fresh execution coverage. A shared-cache stale dependency caused the first separate benchmark compile to fail; refreshing the workspace crate outputs and compiling client plus example together succeeded without reserving another number.

## Artifact identity and evidence

- Build 124 client SHA-256: `5D9B151E286420DD181EF4060B2EBD93FBBFD085791018692A0871DBE3D1FCCC`.
- Build 124 benchmark SHA-256: `13BBB862A49EFECD58A9E1177ECB3063C1522F3C0E8BB9B2E4B44724C33B43AD`.
- Build 123 benchmark SHA-256: `2412030BB3BF4FBDB1CEFFB671164B3920B12009E88851C9A31737CA4CFD4941`.
- Installed runtime: `C:/Users/Weaver/AppData/Local/SolarityClient/testing/solarity-runtime.exe`.
- Raw evidence under the canonical checkout: `target/build124-{before,after}-{one,two,long}.{csv,json,log,err.log}`.
- Analysis: `target/build124-summary.json`, `target/build124-residency.json`, `target/analyze-build124.py`, `target/report-build124.py`.
- Reproduction: `target/run-build124-comparison.ps1`; compile evidence: `target/build124-package.log`, `target/build124-rebuild.log`.
- The prior installed Build 123 runtime is preserved at `target/solarity-runtime-build123.exe` with its prior build-info file.

## Live-session comparability correction

The user reports approximately 120 FPS in Build 123 at 2K and identifies Soap
as the default test character. The installed Testing profile uses shadow
quality 5, farclip 1277, and environmentDetail 1.5. The fixture above used
shadow quality 2 and a different view-distance profile. Its 400+ FPS values
therefore do not represent Soap's gameplay conditions, and the measured
percentage must not be extrapolated to the user's 120 FPS baseline.

A meaningful live comparison remains outstanding: Soap, identical location
and camera, 2560x1440, the same actual graphics settings, all visible scene
resources ready, and comparable population. Desktop capture failed on this
host, so no visual entry validation or live Soap FPS result is claimed.

The subsequent source fix retains the loading card until the camera's required
terrain window has completed GPU publication and visible ground-detail jobs
have completed. Completed tiles finish GPU admission behind the loading card;
normal gameplay retains incremental admission. Map-transfer acknowledgement
continues to use its original prerequisites, while player completion waits for
the scene. Loading-card completion also checks current readiness, preventing
old completed progress from releasing a newly unready scene. These changes
are subsequent to the installed Build 124 artifact documented above.
