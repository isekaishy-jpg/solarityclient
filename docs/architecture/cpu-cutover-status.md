# CPU architecture cutover status

The user selected a direct cutover. The rollback tag is
`rollback/pre-cpu-cutover`, at `3be425958fb641aff014e218121a2c0b2e86b802`.
The complete requirements remain in the [frame-job design](cpu-frame-job-design.md),
[composition design](cpu-crate-composition-design.md), and
[cache/residency design](resource-cache-residency-design.md).

## Connected source changes

- Replaced both Rayon pools with one persistent protected/flexible worker set;
  the configured total thread count is preserved and its resolved split logged.
- Added worker startup handshakes and x86-64 MXCSR control matching. Existing
  kernel arithmetic and stock reciprocal estimates remain unchanged.
- Added reusable owned frame batches, incremental job publication, independent
  result consumption, panic-safe state return and durable condition predicates.
- Removed the borrowed synchronous frame API. Unit poses now overlap subsequent
  admission/traversal; geometry starts during ordered placement traversal.
- Shared immutable M2 GPU source generations and retained particle/ribbon draw
  templates remove renderer registry borrows from geometry kernels. Effect
  simulation state and final transparent/publication order retain their owners.
- Added controlled tests for independent readiness, incremental producer restart,
  bounded batch admission, state return on worker/consumer panic, shutdown and
  rejecting a worker join that would deadlock its lane.
- Connected durable CPU completion notifications to a Windows event/timer/message
  wait bridge. Frame pacing, minimized service and cinematic deadlines use it;
  SDL remains the input owner and gameplay keeps its existing ordered cutoff.
  Native failures are explicit, producer faults are latched, and signal ownership
  survives late producers. SDL watches precede queue insertion, so the adapter
  retains a finite maintenance rescan (at most 16 ms before scheduling delays).
- Terrain streaming now installs the current camera window before offering a
  decoded tile for GPU admission. Regression coverage rejects both an unpolled
  completion and an already-staged old-window tile after movement.

## Still required for the complete cutover

- General dependency templates, typed/generational outputs and external ready
  tokens; cancellation, priority propagation and main-only continuations.
- Node/result/scratch byte reservations and accounting. Current admission limits
  count background tasks and frame batches separately; they do not bound a
  batch's job count or total retained bytes.
- Extend the native bridge to loading/GPU-slot waits and main-ready continuations.
  Current frame consumers still wait at their necessary consumption boundaries.
- Cross-domain terrain/WMO/UI/rendering overlap and phase-specific M2 demand;
  ordered receiver lighting and final geometry reclamation still have barriers.
- Shared asset request identity/lifecycle, stock-evidenced animation demand and
  retention, derived cache invalidation, byte-budgeted residency and GPU retirement
  from the resource design.
- Full causal wait/queue attribution, overhead/scaling checks, and matched
  movement/loading/live measurements. No FPS gain is established by compilation
  or synthetic correctness tests.

These are remaining implementation requirements, not optional deferred scope.
No numbered Testing build has been produced from this in-progress cutover.

## Checkpoint validation

The native-wakeup/current-window checkpoint passed formatting, workspace Clippy
and all 1,427 tests, with 33 ignored, on 2026-09-15. Coverage adds native arm and
notification races, late-producer lifetime, SDL event preservation, already-seen
Windows input, latched native failure, frame-limiter failure propagation and
old-window rejection before GPU admission. Logs are in ignored
`target/cpu-wakeup-workspace-tests.log` and `target/cpu-wakeup-clippy.log`.

### Initial owned-batch checkpoint

On 2026-09-15, formatting and workspace Clippy passed. The full
`cargo test --workspace --all-features` run passed 1,417 tests with 33 ignored.
This includes pose parity across camera/override changes and geometry parity
against serial traversal during motion, visibility changes and effect-state
cleanup. Both geometry paths use the retained draw-template constructor;
renderer validation and stock rendering fixtures cover its packet behavior.
The initial full run exhausted disk space while linking; after removing only
generated debug incremental caches, the complete rerun passed with
`CARGO_INCREMENTAL=0`. Logs remain in ignored `target/cpu-cutover-tests.log`
and `target/cpu-cutover-clippy.log`. These checks establish correctness coverage,
not completion of the remaining architecture or a measured performance gain.
