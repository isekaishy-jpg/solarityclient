# Testing Build 174: shared continuations and parallel world recording

Build 174 packages source `3426e8f18342d0567b51a7414a14427f1390d5d2` from
`perf/critical-frame-work`. It was installed on 2026-09-20 at 23:28 EDT.
The installed executable and packaged artifact have matching SHA-256:
`233912255A558D6CFAD9DC47DD1233AF16575BC1BEE13E2BBF2DAF30ACCF78CB`.
The reported dirty flag covers the build-number reservation, the only tracked
change during packaging. Installation preserves four CPU workers, capacity 256,
two network workers, 2560x1440 fullscreen-windowed mode and GPU 0. The Testing
shortcut target and installed build/revision were verified. No interactive
client was launched.

## Connected changes

Admitted CPU services can suspend on a discovered source dependency without
occupying a worker. They retain ownership, result delivery and live demand;
cancellation and shutdown resume contextual cleanup. Terrain MDDF/MODD and
ground-detail models join shared M2 authority. Terrain and GameObjects share
one WMO root/group producer and publish only complete consumers. WMO work yields
between groups; withdrawal finishes an already-owned shared source without
continuing the cancelled scene. Retirement runs in bounded worker steps.

World command encoding uses owned contiguous ranges and independent secondary
command pools on CPU workers. Main records the UI/compositor before the necessary
join and ordered submission. Generated stock textures retain staging until GPU
completion instead of synchronously waiting for each transfer. Frame lifetime,
draw order and failure/unwind reclamation remain explicit. The changed areas
have folder-backed preparation, source, recording and publication modules.

## Validation and limits

- Formatting, workspace Clippy with all targets/features and warnings denied,
  and all 1,676 tests pass; zero fail and 33 existing tests remain ignored.
  Only leading documentation comments changed after the suite; final formatting
  and whitespace checks pass.
- The optimized candidate completes 1,792 crowded frames on 1/2/4/8 workers,
  plus an 896-frame F10 replay. The preserved earlier binary also completes its
  896-frame F10 replay. No replay crashes or stderr errors occur.
- These are correctness/progress checks, not evidence of a live FPS gain.
  The F10 candidate averages 29.037 ms/frame versus 24.097 ms for the earlier
  binary in that pair. Command-recording elapsed time is 2.483 versus 2.649 ms,
  including necessary joins; queue-present time and unchanged CPU work also rise.
  See the [measurement details](cpu-world-recording.md#combined-source-qualification).
  Hidden presentation and the fixture's below-terrain travel path limit attribution.

The complete CPU cutover remains unfinished. Main M2 admission, remaining
cross-resource consumers and demand, requested animation loading and complete
cache/working-set ownership remain required in the
[cutover status](cpu-cutover-status.md). This package does not relabel those
requirements as optional performance tuning.
