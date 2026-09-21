# Testing Build 175: consolidated CPU cutover

Build 175 packages `f14d27ab70af670dfe47266042f1e4295e4fe47c` from
`perf/critical-frame-work`. It was installed on 2026-09-21 at 03:44 EDT.
The packaged and installed executables have matching SHA-256:
`0463F7EC8DBF0C43CC22E12F960BC96EFD5FE2722BE1CA3C37EBE09A83E0E61A`.
The embedded dirty flag covers only the build-number reservation; the source
and index remained unchanged throughout packaging.

All three cutover histories and the paused effect-loader changes now live on
one branch. The temporary composition/admission branches and worktrees are
retired. Their logs and research artifacts are preserved in the shared Git
recovery directory, along with the original paused-work stash identifier.

The package includes scoped source demand, encoded read ownership, retained M2
payload accounting, mounted-rider pose work, late-discovered bone continuations,
native screenshot waits and resumable shared effect sources. Effect loading
separates model resolution from derived preparation, retains charged completion
records through GPU warmup and propagates producer/admission failures instead
of silently omitting authored effects. World, event and source handling use
separate modules.

Formatting and full workspace Clippy with warnings denied pass. All 1,706
workspace tests pass, including doc tests, with zero failures and 33 existing
ignores. The first test compilation exhausted disk space; the full rerun passed
after removing the retired composition compiler cache. Evidence is in ignored
`target/consolidation-{clippy,test-final,package}.*.log`.

Installation preserves four CPU workers, capacity 256, two network workers,
2560x1440 fullscreen-windowed mode, GPU 0 and the normal F10 profiler toggle.
The installed identity, artifact hash and Desktop shortcut were verified. No
interactive client was launched and no new live FPS gain is claimed.

This completes branch consolidation and packaging. Remaining CPU architecture
requirements are still recorded in the [cutover status](cpu-cutover-status.md).
