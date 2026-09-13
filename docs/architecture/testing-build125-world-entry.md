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

No live Soap FPS gain is claimed. The prior offline fixture used different
graphics settings and does not establish improvement over the user's roughly
120 FPS Build 123 session. A matched live session is still required.

Evidence in the canonical checkout: `target/world-entry-target-test.log`,
`target/world-entry-final-checks.log`, `target/build125-world-entry.log`, and
`target/build125-install.log`. Per-session timing logs are in the installed
Testing client's `logs` directory.
