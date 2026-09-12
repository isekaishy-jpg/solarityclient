# Live CPU and sampled GPU timings

These diagnostics locate millisecond-scale costs in the actual client workload.
The 1280x720 offline world replay does not establish performance for a populated
2560x1440 session. Profile the same scene, resolution, settings, population and
camera that exhibit the problem before choosing a performance change.

## Enabling a diagnostic run

Normal launches leave the diagnostics disabled. With an instrumented Testing
build installed, run:

```powershell
./scripts/profile-test-client.ps1
```

The script uses the installed launcher's configuration and timestamped log,
temporarily sets both timing environment variables, and restores its caller's
environment on exit. `-Mode cpu` and `-Mode gpu` select individual diagnostic
groups. The default Testing launcher is unchanged. Direct executable launches
can set `SOLARITY_FRAME_TIMINGS=1` and/or `SOLARITY_GPU_TIMINGS=1`; unset the
variables to disable them (presence enables profiling, including a value of 0).

CPU summaries include `live client frame`: platform input, session/world service,
presentation, and the frame limiter. Existing nested scopes split residency,
movement, UI, animation, scene preparation and renderer submission. Nested scopes
must not be added together. Vulkan CPU summaries now expose `fence_wait_*` as a
subset of `wait_write_*`, distinguishing completion waits from mapped-buffer
writes and resource preparation. Acquisition, command recording, submission and
presentation retain their separate timings. These are host timings, not GPU pass
durations.

GPU summaries report `profiled sampled GPU world frame`, with the actual pixel
extent, sample count, unavailable count, mean/maximum elapsed time and ten named
intervals. `worst_frame_phases_ms` uses the same order as `phase_mean_max_ms` and
records the phases of the worst sampled frame, rather than combining independent
phase maxima. The unified renderer is also used for Glue model presentations;
separate world entry/loading from steady world intervals when reading the log.

The intervals cover shadows; uploads/sky/distant terrain; terrain; WMO; ground
detail; opaque liquids; the interleaved M2/transparent-liquid/particle/ribbon
queue and underwater particles; screen effects; UI; and the final transition
including requested capture work. Authored draw order is unchanged.

## Overhead and ownership contract

- Disabled GPU profiling allocates no pools, records no timestamp/reset commands,
  reads no query results and calls no profiling clock. Its branch checks sit at
  phase boundaries, never inside draw loops. Disabled unified-renderer CPU
  profiling now avoids its previously unconditional clock reads.
- GPU profiling samples one frame in sixteen. Each used frame slot lazily owns
  one eleven-query pool. Fixed arrays hold results and aggregates; query storage
  is not allocated per frame.
- Collection follows the slot's existing fence wait. It introduces no fence wait,
  queue/device idle, readback buffer, extra submission or pipeline barrier.
  `vkGetQueryPoolResults` uses 64-bit results without `WAIT`; `NOT_READY` discards
  that sample and increments `unavailable`, without retrying on the render thread.
- A pool becomes pending only after successful queue submission, including when
  subsequent presentation fails. Unsubmitted recordings are never read. Pool
  reset is recorded outside rendering, before all writes. Pool destruction shares
  the frame slot's existing retirement contract, including swapchain rebuilding.
- Timestamp units and valid counter bits come from the selected graphics queue.
  Conversion subtracts integer ticks with the queue's wrap mask before converting
  to milliseconds. Explicit GPU profiling fails with a diagnostic if that queue
  lacks a usable timestamp clock; ordinary rendering requires no profiling support.
- GPU reports are emitted at most once per two seconds, on sample collection.
  Existing CPU reports are bounded per scope. Reporting still has formatting and
  log-output cost, and timestamp commands have driver/GPU cost; enabled overhead
  must be measured with otherwise identical runs. Do not claim literal zero cost.

Timestamp stages follow the [Vulkan timestamp query contract](https://docs.vulkan.org/spec/latest/chapters/queries.html).
The first marker is TOP_OF_PIPE and subsequent markers are BOTTOM_OF_PIPE. These
are elapsed queue intervals, not isolated shader execution times: pipeline
overlap, scheduling and semaphore dependencies can affect attribution. No
barriers are added to force isolated timings. Sampled maxima can miss unsampled
spikes or periodic workloads; CPU frame summaries still observe every frame.
The last in-flight samples may be discarded at shutdown or swapchain recreation.

Use a separate run for screenshots/video, and do not compile during measurement.
Compare profiling disabled and enabled on the same executable before treating
small differences as workload costs. GPU-only mode separates query overhead from
the existing CPU profiling/reporting overhead.

## Validation on 2026-09-12

Formatting and workspace Clippy with all targets/features and warnings denied
passed. Workspace tests ran with GPU timings enabled: every target except one
terrain-publication fixture passed initially; after the concurrent masked-terrain
change's fixture correction, the complete runtime library rerun passed (334
passed, 18 intentionally ignored). Rendering passed 33 unit and 187 integration
tests with queries enabled. Timestamp arithmetic covers narrow-counter wrap,
64-bit wrap and small differences between large epochs.

An optimized client build and a separate live-loop launch passed. The latter
reported the four `live client frame` phases and shut down normally on window
close. It exercised Glue without logging into a populated world, so it validates
instrumentation wiring rather than the reported live-world performance problem.
Testing Build 000122 was built separately and does not include this
instrumentation.

The overhead replay used one frozen optimized executable, GTX 1070 at 2560x1440,
uncapped presentation, shadow quality 2, and 1,200 frames per phase. Six measured
phases covered stationary/orbit/pointer/travel/settled workloads; initial streaming
was excluded from the means. Its executable SHA-256 was
`E92723AE64693B3A176F2B491C2DC97E233624867A36582A65F93FA841266556`.
Local CSVs, logs and the analysis are retained under
`target/gpu-timings-overhead-*`.

The first off/GPU/both sequence averaged 2.952/2.943/2.977 ms respectively. A
second sequence was confounded by the separately installed client closing. A
final both/GPU/off sequence without that process averaged 2.975/2.743/3.128 ms.
The disabled final run itself reached 94 ms, and combined profiling reached
44 ms. These variable runs establish neither a precise overhead estimate nor
a bound on diagnostic frame spikes. There was no repeatable millisecond-scale
regression from enabling profiling; apparent speedups must not be attributed to
the instrumentation. All GPU reports had ten valid intervals and zero unavailable
samples. A quiet, representative live-world on/off comparison remains necessary
before claiming negligible enabled overhead in that workload.
