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
site, 8,192 retained events and 32,768 causal trace rows per thread per snapshot
interval. Accumulators include fixed histograms. On this 64-bit build the trace
records reserve another 4.25 MiB per registered thread, plus an equal writer
buffer during capture. Together with the existing accumulators/events, this is
about 5.6 MiB retained after first capture and 12.2 MiB while capturing, per
registered thread, excluding bounded asset-name strings. These are reserved
buffer capacities, not a measurement of committed working set. Historical capture writers
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

The original `test-client` recorder experiment on the development i5-9600K,
before the causal trace pass, measured three rounds:

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
- `.trace.csv`: sampled parent/consumer chains, owner requirements and outputs,
  source identifiers, ordinary slow spans and submission-linked GPU results.
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

## Deeper Orgrimmar attribution

The user reports 180–190 FPS in stock Orgrimmar (about 5.3–5.6 ms/frame).
This is a reference observation, not a matched measurement against our route.

F10 now also records:

- `m2.work.*`: on detail frames only, placement visits and early rejections;
  camera, primary/environment-shadow, light, callback-owner and particle-owner
  demand; full palettes, batch hits, actual mesh/shadow/particle/ribbon output
  owners, CPU publication and palettes producing no draw output. Reasons
  overlap. A palette without a draw can still have valid CPU consumers; it
  must not automatically be labelled wasted. CPU output observes event and
  attachment list publication, not every possible callback side effect.
- `m2.pose_batch.prepared/unconsumed`: completed batch results still available
  after traversal, distinguishing work prepared from work actually consumed.
- `m2.topology.*`: ordinary-frame event rows identify local, creature, remote,
  GameObject, static-streaming and retirement mutations and rebuild size.
  Addition/removal counts describe physical placement records, not unique
  semantic identities; a retained instance can be moved through replacement.
  Retirement-start membership and expired-group counts have explicitly different
  units. Static retirement counts refer to owner keys. A removal event with zero
  records still means that code invalidated topology. Initial scene construction
  or starting capture after a mutation may yield a rebuild without prior cause
  rows. No diagnostic-only full-scene identity comparison is performed.
- World-update phases separate GameObject admission, template/network query
  admission, transport placement, animation/collision transforms, collision
  registration, remote movement, effect publication and local appearance.
  Sampled GameObject counters compare already-read presentation/placement
  inputs; collision counters distinguish visited, unchanged and re-registered
  owners. No extra projection or collision query is performed for measurement.
- `frame.cpu_work`, `world.service_cpu`, `m2.prepare_cpu`,
  `rendering.vulkan.world` and CPU batch spans pair coarse wall timings with
  `.cycles` and `.cycles_available`. Windows supplies charged current-thread
  cycles without opening handles. Slow spans retain cycle event rows even on
  ordinary frames; other values aggregate normally. These cycles are not time
  and are not subtracted from wall duration. They help distinguish a long span
  with little charged work from sustained execution; they do not provide a
  scheduler context-switch trace. A batch owner thread's cycles do not include
  sibling workers. `cpu.batch.dispatch_wait` separately records time until the
  admitted batch closure starts executing on the pool.

Inner placement accounting uses stack counters and publishes once per sampled
frame. Only infrequent topology causes and slow coarse-span cycles expand the
ordinary event stream. The same bounded, nonblocking recorder and background
writer remain in use. Capture epochs reject old delayed observations after a
restart. There are no new graphics queries or device waits. Observer cost is
nonzero: the optimized profiler example includes cycle-span measurements and
the ignored `benchmark_placement_accounting` runtime test measures local
disabled/enabled counter bookkeeping separately from publication.

Optimized September 14 measurements on this machine: cycle spans were
1,180–1,217 ns enabled and 4.2–4.8 ns disabled across three rounds. A standalone
optimized harness including the actual placement-accounting module measured
2.21 ns/visit disabled and 6.61 ns/visit sampled, including counter publication
once per 3,000 visits. These are observer microbenchmarks, not measured changes
in game FPS or guarantees about complete-capture overhead. Formatting, workspace
Clippy and the complete test suite passed (1,396 passed, 30 explicitly ignored).


## Causal trace pass after Build 136

The new `.trace.csv` complements aggregates and slow events. It answers which
operation requested work, which thread executed it, which frame consumed it,
and which submission produced a GPU observation. Existing aggregate columns
remain compatible. F10 controls all collection; there is no additional switch.

### Coverage audit and implemented links

| Boundary | New evidence | Remaining interpretation limit |
| --- | --- | --- |
| Frame, input, world service, rendering | Sampled coarse scopes/phases have explicit parents and monotonic start times; workload values attach to their current span | A parent denotes causality; deferred children need not fit inside its elapsed interval |
| Network to world/ECS | Received packet admission, numeric opcode, queued context and application edge; sampled field/transform/movement/removal scopes identify owner GUIDs and received field indices | No payloads, names or credentials; later owner matching is evidence of shared identity, not proof that a specific field caused every later operation |
| CPU and assets | Task admission, inherited execution context and result-consumption edge; frame batch context reaches worker items | No detached jobs or additional worker joins; descendants can complete in later ordinary frames |
| M2 admission to effects/packets | Per-placement owner/source, exact sampled requirement mask, source-path dictionary, CPU bone demand, deferred palette decision, unit-pose request/execute/consume and geometry execution/publication links, output counts | Placement/source ordinals are local to the originating preparation span; required CPU/shadow consumers are not automatically waste |
| M2 to rendering/GPU | Prepared frame consumed by world submission; GPU query slot retains submitting context through existing fence retirement | GPU durations use a separate clock; `start_ns` for GPU rows is observation time, not GPU start time |
| UI, text and startup | Callback arena owner IDs, sampled glyph cache hits/misses and coverage bytes; existing snapshots/layout/glyph/upload scopes participate in the same tree | No text contents; ordinary slow callbacks have an owner but no complete unsampled ancestry |
| Audio and media | Existing sound/archive/decode/cache/voice/movie scopes inherit parent context; decoder jobs retain CPU submission/consumption provenance; sample cache outcomes are explicit | Native mixer/codec internals and movie-only GPU blits remain outside detailed tracing |
| Movement/collision, terrain/WMO, resource lifetime | Existing coarse stages and numeric cause/workload counters join the sampled tree, including publication/topology and resource observations | The trace does not instrument every third-party call, infer cache invalidation correctness, or prove stock visibility |

The existing all-nine-crate timing coverage was reviewed by boundary, rather than
adding an unconditional timer to every function. This pass changes observability,
not visibility, animation, networking, caches, scheduling or draw decisions.

### Trace format and sampling

CPU rows have `span_id`, `parent_id`, optional `related_id`, `origin_frame`,
`completion_frame`, `start_ns`, `duration_ns`, `kind`, static `label`, numeric
`owner/reason/value`, and an optional asset `name`. IDs are capture-process unique.
CPU times use one monotonic origin. `span` and `phase` rows describe elapsed work;
`admit` rows establish transferred operations; `link` rows connect consumers to
those operations; `value` rows attach workload facts. `asset` rows map an M2 source
ordinal to its stock archive path. Paths are bounded to 512 bytes and emitted once
per source in a sampled preparation, with no archive lookup for diagnostics.
`timing` rows attach already-measured CPU durations, including Vulkan wait/upload/
acquire/record/submit/present phases and CPU dispatch/queue residence. Their start
timestamp is publication time, not a reconstructed start of the measured interval.
They do not participate in exclusive-wall subtraction. Fence wait is contained in
slot wait/upload, and charged scope durations repeat their enclosing spans; neither
may be added again to a frame total.

Full chains originate on the existing one-in-128 detail cadence. Explicitly
propagated workers retain their original lane even if completion occurs during
another frame. Async instrumentation scopes cover individual polls, never parked
I/O; packet and CPU-task envelopes provide the cross-thread transfer links.
Ordinary scopes at least 2 ms also produce `slow` rows with precise timestamps.
These deliberately have no inferred parent chain. A capture can therefore retain
a rare startup hitch without pretending every ordinary operation was traced.
An explicit job-consumption edge does not enable full tracing of the remainder
of an ordinary consumer frame. A transfer admitted before capture started also
has no recorded request ancestry. These gaps are not inferred from temporal proximity.

M2 placement span `owner` is placement ordinal plus one, and `reason` is source
ordinal plus one. Qualify both by their ancestor `M2 frame preparation` span: a
single live frame can prepare multiple independent model scenes. `m2.requirements` attaches this bit mask to that placement:
0 admitted, 1 visible mesh demand, 2 primary shadow, 3 environment shadow,
4 light owner, 5 callback owner, 6 particle owner, 7 full palette demand,
8 batch hit, 9 shadow output, 10 observed CPU output, 11 queued visible geometry.
Flags describe actual branch decisions and may overlap. `m2.owner.guid` connects
dynamic placements to sampled ECS ownership. A GUID is not a lifetime guarantee;
removal/recreation still needs its surrounding admission evidence.

The recorder adds a separate bounded 32,768-row trace buffer per registered thread.
Recording uses the same try-lock policy; writer contention and capacity exhaustion
increment `dropped_trace_rows` rather than blocking producers. Only source dictionary
rows allocate strings, in the detail lane; ordinary events use static names and
numeric records. No GPU readback, wait, device idle or queue submission is added.
The background writer performs CSV formatting and file I/O. Ending a capture can
leave links to unfinished work; missing references must remain visible in analysis.

### Reading chains

```text
python scripts/analyze-trace.py <capture.trace.csv> --frame <origin-frame> --json target/chain.json
```

Without `--frame`, the helper selects the slowest sampled live frame; use
`--after-frame` to exclude startup. It streams the input and retains only the chosen
origin frame plus referenced ancestors. It reports missing references, parent spans,
model-source work and dependency/GPU records. Exclusive wall time subtracts only
intersecting same-thread child spans. It is not charged CPU time, and does not
subtract asynchronous worker or GPU work. Main-thread ordered preparation and
worker pose/geometry time remain separate in the model table. JSON retains
requirement masks, numeric observations and phase rows for further investigation;
the table decodes overlapping demand reasons by model source.

Reproduce the observer experiment with:

```text
cargo run -p solarity-profiling --profile test-client --example causal_overhead -- target/causal-overhead
python -B -m unittest discover -s scripts/tests -p test_analyze_trace.py
```

The bounded experiment keeps sampled rows below capacity and excludes cold setup.
Three optimized observations after reducing inactive-record initialization:
disabled scope 12.64-12.80 ns; disabled context 3.96-3.99 ns; ordinary scope
116.33-117.29 ns; unsampled inner owner 20.75-20.94 ns; sampled coarse span
183.50-204.50 ns; sampled owner with one phase/output 481.95-530.95 ns. These are
nonzero observer costs, not client FPS measurements. The older profiler experiment
used smaller records; the new disabled cost must not be described as unchanged.

Received field indices describe submitted updates, not proof that the old value
differed. They are observed in the existing iterator only on sampled chains;
field values and packet payloads are never copied into diagnostics.

### Validation

The workspace formatting, all-target/all-feature Clippy and all-feature tests
passed: 1,412 tests passed, none failed, 33 explicitly ignored. A final focused
CPU/profiling run passed 17 tests, including the added real-executor cross-frame
provenance test. That test distinguishes join admission from actual result
consumption and verifies consumption follows worker output. The Python analysis
tests cover overlapping intervals, missing dependencies, GPU separation and
source ordinals reused by independent model scenes (three passed).

The first optimized hidden city capture (`capture-1789456169776-1`) contained
220,250 trace records over 37.98 seconds, with zero missing references across
the whole file and zero dropped samples, events, traces or capacity overflows.
It used Soap at map 1, position 1515.34/-4417.27/18.0499, 2560x1440, Ultra
shadows, four CPU workers, seven 512-frame phases and 120 authored Goblin NPCs.
This is an offline validation scene, not the live server population: networking,
native audio and the gameplay movement solver are not exercised by this harness.

In origin frame 3073, M2 preparation was 4.569 ms; the existing renderer event
stream attributed 3.894 ms to queue presentation. The new model chain identified
five visible floating-ember owners producing 10,736 particle vertices, with
0.509 ms summed geometry worker time for that source. Eighty-one unit palettes
were prepared and all consumed. These observations demonstrate separate work
and wait chains; they are not an FPS improvement or proof that hidden-window
presentation costs match the live client. The Vulkan phase gap found in this
capture led to the `timing` records described above.
