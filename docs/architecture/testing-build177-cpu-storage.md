# Testing Build 177: connected CPU storage admission

Build 177 packages `d1e90f309176e3f0295ba5df88701b281ff921be` from
`perf/critical-frame-work`. The user requested a fresh Testing package after the
accumulated cutover changes and startup of the local playerbots server.
This includes the startup/UI source work since Build 176, namespace ownership,
connected frame inputs/returns, model preparation storage, pose/receiver metadata,
and the latest admitted pose inputs and collective full/named pose outputs.

The optimized `test-client` compilation passed in 7m24s under the standard
exclusive package lock. One number was reserved. Source and index stayed frozen
at the revision above; `dirty=true` covers only the `BUILD_NUMBER` reservation.
The packaging transcript is `target/build177-package.log`.

Installation used `scripts/install-test-client.ps1 -SkipBuild`, retaining four CPU
workers, capacity 256, two network workers, 2560x1440 fullscreen-windowed mode,
GPU 0 and the ordinary F10 profiler toggle. The installer applied its standard
first-run reset. Packaged and installed executable identities and SHA-256 match:
`C740E56C8234E04FEF13A04B8E145621D2BD16EBAC48308E2E726599BE170CA0`.
The Desktop Testing shortcut targets the persistent installed launcher.

The latest source batch passed 564 rendering/runtime library tests with 30 existing
ignores, including a final runtime rerun after the reservation-release correction.
All-target/all-feature Clippy with warnings denied, formatting and diff checks
passed. See the [cutover status](cpu-cutover-status.md) for individual batch evidence
and remaining requirements. No interactive client or FPS comparison was run.

The existing playerbots testing configuration was started hidden with its MySQL
and auth services. `Server.log` confirms mod-playerbots initialized and worldserver
ready; `Auth.log` registers GameMap Playerbots Test. Listeners were verified at
127.0.0.1 ports 3306, 3724 and 8086. Existing configuration enables automatic login
with 24 bots. Startup output is in
`C:/GameMapRuntime/playerbots-logs/*-start-20260922-173224.*.log`.

At the user's request, stale generated output was removed from the shared target.
Cleanup removed 328 older debug test executables/symbols, the debug examples and
inactive incremental directory. It recovered 28.38 GiB; debug output decreased
from 47.66 to 19.28 GiB, preserving current dependency artifacts, today's tests,
evidence logs, the active package and the installed client. C: had 36.22 GiB free
after installation. The exact removal manifest is
`target/cleanup-20260922-manifest.txt`.

This package does not declare the full CPU cutover complete.
