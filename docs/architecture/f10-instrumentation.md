# Persistent F10 performance captures

Press **F10** once to start and again to stop. It works in Glue, legal dialogs,
loading screens and the world. Repeat/release events are consumed without
toggling again. The client log reports the transition and saved path. Stopping
requests a final background write; client shutdown also finishes an active capture.

Files are saved below the active profile root in `Profiles`. For the installed
Testing client this is `%LOCALAPPDATA%/SolarityClient/testing/Profiles`. No login
credentials, packet contents, chat, tooltip strings or account names are recorded.
Existing `SOLARITY_FRAME_TIMINGS`/`SOLARITY_GPU_TIMINGS`/`SOLARITY_UI_TIMINGS` launch flags start the same
complete capture automatically; F10 can stop it. Separate startup-only profiler
implementations and their synchronous timing logs have been removed.

Use Soap and a repeatable route for movement tests. Keep camera, loaded scene,
resolution and graphics settings matched when comparing captures. Include a
stationary period before and after movement. A changing scene is useful evidence,
but it is not a matched before/after FPS comparison.

## Coverage

| Domain | Measurements |
| --- | --- |
| Application | Complete live frame, SDL polling/input, session/world service, presentation, frame limiter and main-thread CPU cycles |
| Network | Active future polls for authentication, realm/world I/O, parsing/serialization, runtime receive/writer tasks, packet application and received body bytes; parked I/O time is excluded from poll durations |
| CPU executor/assets | Queue residence, job execution, batch joins, sampled worker items, archive reads, terrain/model/WMO/texture loading and admission |
| ECS/movement/camera | Object mutation, spline and grounded/airborne/swimming updates, world camera solve and terrain/WMO/M2/liquid collision providers |
| M2/world | Residency/topology, dynamic models, pose batches, visibility, per-placement admission/animation, full/sparse poses, CPU consumers, shadows, liquid/fog, particles, material/mesh packets, ribbons, receivers, sorting and light publication |
| UI/text | Glue/FrameXML updates and events, Lua update/event batches, mouse-wheel dispatch, targeted publication, scrolling, glyph/texture geometry, sampled font measurement/rasterization |
| Audio | World/zone sound update, admission, archive/decode workers, cache/resource management, advanced kits, voice collection and sampled native playback/spatial control |
| Movies | Decode/conversion, audio feeding, playback scheduling, upload, command recording and submit/present CPU duration |
| Vulkan CPU | World resource admission, upload, slot/fence waits, acquire, command recording, submit and present; UI-only rendering and mesh uploads; resource retirement |
| Vulkan GPU | Sampled world shadow, upload/sky, terrain, WMO, ground detail, liquids, M2/effects, screen effects, UI and presentation transitions; separate UI-only GPU submissions |
| Workload/lifetime | Draws, bones, particle vertices, resident/dynamic M2 counts, audio voices/cache bytes, selected GPU payload capacities/retirement backlog, resolution, player position and CPU in-flight work |
| Host | Periodic process working set/private bytes and cumulative CPU/cycle counters for every accessible Windows process thread, including native mixer/driver threads |

This instruments subsystem boundaries and selected expensive inner stages. It is
not a function-by-function trace of third-party libraries, SDL's native mixer,
the driver, or GPU shaders. Movie-only GPU blits do not have their own timestamp
series; their CPU submission and waits are recorded. Rust scope durations are
elapsed time, including preemption and any waits inside the scope. Async scopes
measure poll execution only. OS thread counters help distinguish CPU execution
from elapsed waiting without placing a system call around each probe.

## Observer cost and storage

Disabled probes read a relaxed atomic gate; they do not call a clock, allocate,
format strings, register threads or lock a recorder. Value expressions are not
evaluated while disabled. Ordinary captures retain coarse scopes each frame.
Inner-model/font/voice probes and GPU timestamps run on one frame in 128, with
their frame lane recorded separately. Resource registry walks occur only in that
detail lane, under `diagnostics.scene_snapshot`.

Static sites register once. Each measured thread gets fixed accumulators and a
bounded event buffer; no event allocation occurs after first use. Recording uses
a per-thread try-lock: a writer collision drops a sample instead of waiting.
One writer swaps buffers once per second, formats CSVs outside locks, and queries
host counters every five intervals. There is no extra GPU wait, device idle,
queue submission, screenshot or readback for profiling. Timestamp results are
read without `WAIT` after the frame slot's existing fence and carry their
submitting capture generation.

Limits are 1,024 registered metrics, 64 measured threads, 32 named phases per
site, and 8,192 retained events per thread per snapshot interval. Accumulators
include fixed histograms. A registered thread retains about 1.3 MiB while off;
active writer scratch/totals bring it to about 3.7 MiB. Historical capture writers
are joined and released; static registry storage is reused across captures.
Limits are process-wide, so capacity exhaustion is reported explicitly.

Instrumentation is not free. First-use registration/buffer allocation and the
first timestamp-pool allocation are cold capture costs. The first two seconds
are excluded by the analysis helper; new thread/metric registration later in a
capture must also be considered. Reports include registered metric counts by
interval, dropped samples/event rows, capacity exhaustion and writer wall time.
Ordinary versus detail frame timings must remain separate. Outer frame/scope
durations include nested probe bookkeeping; each scope stops its clock before
recording its own final aggregate. Nothing is silently subtracted from durations.

The optimized `test-client` recorder experiment on the development i5-9600K
measured three rounds:

| Probe | Observed time per call |
| --- | --- |
| Disabled duration scope | 3.53–3.71 ns |
| Disabled value | 0.29 ns |
| Enabled ordinary scope | 97.63–111.75 ns |
| Enabled value | 25.64–29.89 ns |
| Unsampled detail scope | 3.53–4.73 ns |
| Sampled detail scope | 98.67–106.29 ns |

These are recorder microbenchmarks, not a claim of zero whole-client overhead or
a gameplay FPS gain. They exclude first-use registration, phase marks, OS calls,
GPU commands and scene snapshots. For scale, 100 enabled ordinary scopes cost
roughly 0.01 ms at these measured rates. Reproduce with:

```text
cargo run -p solarity-profiling --profile test-client --example overhead -- target/f10-overhead
```

## Reading a capture

Each `capture-<timestamp>-<generation>` produces:

- `.csv`: one-second per-thread/per-scope aggregates, with ordinary/detail lanes.
- `.summary.csv`: cumulative aggregates with count, total, mean, histogram
  percentile upper bounds and exact maximum. Scope totals include child scopes;
  do not add overlapping scopes, worker durations, GPU duration or nested waits.
- `.events.csv`: every completed live frame, scopes at least 2 ms, and detail-frame
  workload snapshots, with completion frame numbers. Worker/GPU completion frame
  means observation time, not necessarily the frame that submitted the work.
  `elapsed_seconds` identifies the writer interval, not an exact event timestamp.
- `.resources.csv`: process/thread CPU and memory snapshots plus registration
  counts. Thread CPU values are cumulative; compare differences between samples.
- `.txt`: build identity, workers, capture health and interpretation notes.

All duration values are nanoseconds. Workload values are unsigned integers;
`*_f32_bits` values preserve signed floating-point coordinates as their IEEE-754
bits. Workload percentile fields are not useful time statistics; use values,
means and maxima. Buffer overflow can lose events, so inspect health first.

```text
python scripts/analyze-profile.py <path-to-primary-capture.csv>
```

The helper prints frame percentiles, early/late frame trends, overlapping scope
totals, memory growth and sampled positions. The full CSVs remain available for
correlating a hitch with work on other threads. Measurements collected after a
writer error are unavailable; diagnostic failures are logged and do not replace
gameplay errors.

## Ownership

`solarity-profiling` is a shared observability dependency used by all nine existing
crates. Runtime owns F10 and capture lifetime; the recorder owns bounded storage;
the writer owns file I/O; rendering owns GPU queries under its existing fences.
The recorder, writer/controller and host counters use folder modules with narrow
facades. New feature work can add static scopes without a new launch flag or
temporary logging implementation.
