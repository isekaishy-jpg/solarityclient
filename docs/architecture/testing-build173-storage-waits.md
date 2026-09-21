# Testing Build 173: effect ownership and native presentation waits

Build 173 packages source `e1ee1b3bc61bb5fb01265e6f6692060d677c44df` from
`perf/critical-frame-work`. Installed through the Testing setup on
2026-09-20 at 20:12 EDT. Installed and packaged executable SHA-256:

`C189349E96709C4FF0E9FDAA4CED3E03F0FAFC0C254DE74ABE7C5FC01B9DDFAD`

Dirty identity reflects only the reserved tracked `BUILD_NUMBER`. Source was
committed and the checkout clean before compilation. Tracked source and index
remained frozen throughout the numbered build; the only resulting tracked change
was the build reservation.

## Connected changes

Particle pool/free-slot allocations, fixed ribbon history and named-bone scratch
carry their CPU byte reservations across admission, worker ownership and retention.
Ribbon insertion preserves its authored history without transient ring growth.

The existing GPU completion service now owns acquisition and device retirement
as well as frame-fence observation. World/Glue, UI/loading and cinematic consumers
service native events during acquisition; world resource/quality barriers and
swapchain recreation retain the same scoped lifetime rules. Failed presentation
cannot leave the device incorrectly marked idle after a possible submission.
Camera sampling and gameplay publication order are unchanged. Pose composition
and renderer presentation are decomposed into folder modules.

See the [implementation and measurements](cpu-effect-storage-and-gpu-waits.md).

## Validation

- Formatting and workspace Clippy with all targets/features and warnings denied
  pass. All 1,659 tests pass, zero fail, and 33 existing tests remain ignored.
- Controlled ownership/failure tests and real GPU UI/cinematic pixel tests pass.
- Seven hidden crowded runs complete 6,272 frames without crashes or storage
  refusal: 1/2/4/8-worker candidates and four-worker control/repeat comparisons.
  Timing results are mixed; some movement phases regress, and long frames remain.
  These results do not establish a large FPS gain or full cutover completion.
- Numbered optimized compilation and `-SkipBuild` installation pass. Revision,
  build number, hash and Testing shortcut are verified. Settings retain four CPU
  workers, capacity 256, two network workers, 2560 x 1440 fullscreen-windowed,
  and GPU 0. No interactive client was launched or interrupted.

The complete cutover remains unfinished. Main-thread M2 admission, remaining
cross-resource request graphs, whole working-set/cache ownership and other
requirements remain explicit in the [status document](cpu-cutover-status.md).
