# Testing Build 176: shared texture source construction

Build 176 packages `ff9751b18ef87645e50a818f14716ea2972c95ab` from
`perf/critical-frame-work`. The user requested this one fresh Testing build after
the accumulated cutover changes, then continued source work without further
client packages. The package includes retained texture admission and namespace
identity, CPU WMO visibility/shadows, shared pending BLP sources, resumable terrain
and scene materials, and appearance/backdrop material construction.

The optimized `test-client` compilation completed in 6m05s. The packaging script
reserved 176, then Windows PowerShell stopped on redirected Cargo stderr before
compilation completed. Compilation resumed under the same exclusive package lock
and reservation; no second number was issued. The source and index stayed frozen
at the revision above. `dirty=true` covers the `BUILD_NUMBER` reservation only.
Evidence is `target/build176-package.log` and `target/build176-compile.log`.

Installation used `scripts/install-test-client.ps1 -SkipBuild`, retaining four CPU
workers, capacity 256, two network workers, 2560x1440 fullscreen-windowed mode,
GPU 0 and the ordinary F10 profiler toggle. Packaged and installed executable
identities, their matching SHA-256 and the Testing Desktop shortcut were verified:
`4C8A654BF9DE8E7998D1630BFB48A24050799307EEA39A543DDF2F8787136988`.

The preceding source validation passed 800 asset/runtime tests with 29 existing
ignores, formatting and all-target/all-feature Clippy with warnings denied.
No interactive client was launched and no FPS comparison was performed.
This package does not declare the [CPU cutover](cpu-cutover-status.md) complete.
