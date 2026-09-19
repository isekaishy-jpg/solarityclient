# CPU consumption and native wait attribution

The Build 155 live review exposed a measurement error: `cpu.frame.result_wait`
included nonblocking probes, ready-result callbacks and result-lease return.
Reclamation similarly included state copying and cleanup in its wait total.
Those old values cannot establish how long main was blocked.

The corrected boundary records a wait only around an actual condition-variable
call and mutex reacquisition. A ready result, unsuccessful nonblocking probe or
already retired phase emits no wait sample. Each native attempt, including a
spurious wakeup followed by another attempt, has its own span. OS descheduling
inside a wait remains included; a wall span is not a CPU-service-time measure.

## Scope meanings

| Scope | Measured work |
| --- | --- |
| `cpu.frame.result_wait`, `cpu.load.result_wait` | Native condition wait for a requested node, including lock reacquisition. |
| `cpu.frame.reclaim_wait`, `cpu.load.reclaim_wait` | Native condition wait for terminal phase publication/admission release. |
| `cpu.frame.consume`, `cpu.load.consume` | Ready callback and result-lease return, including callback unwind. |
| `cpu.frame.reclaim`, `cpu.load.reclaim` | Terminal outcome inspection, ordered input return and phase cleanup. |
| `frame_pipeline.cpu_result_pending`, `frame_pipeline.cpu_reclaim_pending`, `frame_pipeline.main_ready_pending` | Coordinator interval while a prerequisite is pending; native input servicing can be useful work inside it. |
| `rendering.gpu_slot.pending` | Pending frame-slot coordination, including native service and terminal observation. |
| `rendering.cinematic_source.reader_check` | Shared-reader checks, possibly returning immediately, plus coordination for unfinished readers. |
| `rendering.gpu_completion.host_wait` | Fence backend call on the dedicated GPU completion thread. |
| `rendering.gpu_completion.drain_wait` | Actual condition wait when host ownership still needs draining, including exceptional native return/unwind. |
| `platform.coordinator.wait` | The Win32 multi-object/input wait; signal validation and input pumping are outside it. |

The old `frame_pipeline.*_wait` coordinator scopes are renamed to `*_pending`.
`rendering.gpu_slot.native_wait` and `rendering.cinematic_source.native_wait` are
replaced by the names above. Historic captures retain their old meanings. Do not
sum pending intervals, nested native waits, GPU-host waits and GPU execution as
independent main-frame costs.

## Causal identity

`cpu.phase.wait_need`, `cpu.phase.consume_need` and `cpu.phase.reclaim_need`
link the current consumer to the producing phase. Their `related_id` is the
phase request. Synchronous wait/consume/reclaim spans inherit that phase's
originating capture/frame, even when main observes it on a later frame. Result
wait and consume spans use one-based node index as `owner`; phase waits use zero.
Reclamation's `reason` records the number of returned model/job records.

A result wait's `reason` is a constant-time admission snapshot:

| Value | Pending state |
| --- | --- |
| 0 | Phase work or retirement; no specific node selected. |
| 1 | External prerequisite gate. |
| 2 | Requested node's predecessor. |
| 3 | Requested node is ready for a worker. |
| 4 | Requested node's kernel is running. |
| 5 | Terminal publication. |

This is a state at native-wait entry, not a breakdown of the entire wait. A
prerequisite can become ready and run before the waiter returns. No node-list
scan or new scheduling decision is introduced.

GPU completion carries a copied diagnostic context in its existing single
request cell. `rendering.gpu_completion.request` parents `host_wait` and
`host_return` on `solarity-gpu-completion`. `host_return` is emitted after the
backend stops using the fence and before readiness publication; it does not
pretend to be an exact atomic-publication timestamp. Its value indicates success.
`rendering.gpu_completion.observe` links main back to the request. Successful
finish and final release may each observe the same retained outcome. They are
observations, not two host waits. Ready cinematic readers dispatch no request.

The Win32 native wait's `reason` is the raw API result: zero for completion,
one for the timer, two for input, 258 for timeout and `0xFFFFFFFF` for failure.
The existing result validation and gameplay input cutoff are unchanged.

## Ownership and observer cost

Results enter an unconditional return lease before diagnostic setup. A moved
closure owns that lease during measurement; normal callback return and unwind
both restore payload ownership before the consumption span closes. Cancellation,
dependency failure, output order, native servicing and GPU lifetime rules remain
the same. The result module now separates observation, waiting, consumption,
reclamation and cancellation/disposal into folder children.

Disabled observations use the existing generation guards: no clocks, trace
allocation, TLS entry or formatted logging. Enabled coarse measurements and
sampled causal records still have overhead; no automatic subtraction is applied.
The reproducible `solarity-cpu` example `consumption_overhead` measures the same
ready-result API with capture disabled, ordinary capture and sampled capture.
Its dispatch, setup and producer waits are outside the measured region; it is
an observer experiment, not an FPS benchmark.

The external CPU fixture forces a real gate wait using the phase metadata lock,
then checks pending probes, ready callbacks, unwind restoration, reclamation,
frame/load classification and later-frame provenance. It uses event ordering
instead of sleep or performance thresholds. The GPU fixture checks a controlled
host backend, main/native separation, return-before-observation, repeated result
observation and ready readers that produce neither requests nor drain waits.

## Validation and observer experiment

On 2026-09-19 the full workspace suite passed 1,590 tests, with zero failures and
33 ignored across 94 summaries. After isolating trace assertions from concurrent
unit tests, formatting and full workspace Clippy with all targets/features and
warnings denied passed again, as did the 47 CPU/rendering library tests. Production
source was unchanged between the full suite and that targeted confirmation.
Logs remain in ignored `target/cpu-consumption-verified-*` and
`target/cpu-consumption-postreview-*`. An earlier `cpu-consumption-final-*` test
build was deliberately stopped before finalizing result-lease ownership; it is
not a failing completed test run.

The example compiled with the optimized `test-client` profile and alternated
disabled, ordinary, sampled, ordinary and disabled modes three times. Setup and
64 warmup calls are excluded. Disabled/ordinary iterations numbered 200,000 per
run; sampled runs use 2,000 to stay below trace capacity.

| Capture mode | Runs | Median ns per complete ready-result call | Observed range, ns |
| --- | ---: | ---: | ---: |
| Disabled | 6 | 54.08 | 52.66-61.52 |
| Ordinary | 6 | 187.34 | 180.41-199.77 |
| Sampled | 3 | 331.65 | 322.50-619.65 |

These include result validation, mutex/lease operations and the trivial callback;
54 ns is not an isolated disabled-probe cost. The experiment neither measures
whole-frame F10 overhead nor establishes a before/after runtime improvement.
All nine captures report zero dropped samples/trace/event rows and zero metric
capacity overflows. Raw measurements and capture files are retained under ignored
`target/cpu-consumption-overhead*`. Reductions in the corrected wait counters must
not be presented as faster execution.

This fixes these boundaries, not the entire remaining causal-reporting design.
Complete resource/I/O graphs, ready-delay attribution, working-set accounting,
overhead/scaling checks and matched live movement/loading evidence remain in the
[full cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover).
