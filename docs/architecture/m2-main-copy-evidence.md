# Main-thread M2 copy evidence

The 2026-09-20 baseline at `ee3d3b43` still pays for copying worker results and
large draw records on the main thread. This is an instruction-sampling finding,
not a measured speedup or proof that copies explain the entire live frame cost.

## Method and limits

A hidden, owned `benchmark_world` child used the same 192 authored NPCs and
2K/Ultra scene as the [receiver experiment](m2-receiver-query-experiment.md).
Each of four phases ran 2,048 frames. Profiling capture was disabled. The sampler
observed the child primary thread from 10 to 40 seconds after launch, suspending
it briefly to read its instruction pointer and a bounded stack prefix, then
resuming it before symbol processing. Only this owned child was inspected.

The copy-caller run collected 10,076 samples. Median suspension was 23.4 us and
p95 was 120.6 us. These pauses perturb execution; sampled-run frame timings are
not performance comparisons. Symbols were resolved from the exact baseline PDB
and local modules after the child exited. The first stack word pointing into
the executable is a return-address candidate, not a fully unwound call stack.
Significant publication sites were additionally verified against machine code.

Windows kernel CPU tracing was unavailable to this non-administrator process.
The attempted WPR start failed; no recording session remained active.

## Findings

1,620 samples (16.08%) landed in `VCRUNTIME140.dll` `memmove`. Caller candidates
were at stack offset zero in 1,619 cases and offset eight in one case:

| Caller category | Samples |
| --- | ---: |
| Geometry publication | 709 |
| Upload packing | 253 |
| Command recording | 194 |
| Geometry job reclamation | 96 |
| Receiver preparation | 61 |
| Geometry admission | 60 |
| Draw sorting | 50 |
| Other | 197 |

These categories count sampled copy instructions, not complete function costs.
Another 1,912 samples landed in the NVIDIA driver. Private driver symbols were
not available: a nearest exported-symbol name with a large displacement does
not identify the actual internal function or prove a particular Vulkan call.

In `GeometryOutput::publish`, baseline machine code establishes these sites
(preferred image base `0x140000000`; function starts at `0x14069cd10`):

| Samples | Return address | Verified copy |
| --- | --- | --- |
| 325 | `0x14069cffd` | Complete palette, count multiplied by 64 bytes |
| 150 | `0x14069d6c2` | 432-byte visible draw record |
| 145 | `0x14069d1b0` | 360-byte shadow packet portion, 432-byte source stride |

The palette source already remains owned by completed `GeometryJob` records.
Main publication nevertheless concatenates every retained palette into another
vector, after which upload serializes every matrix again. Draw packet iteration,
relocation and sorting also move substantial records. These are concrete output
ownership targets; they do not justify replacing the scheduler or relaxing stock
callback, simulation, shadow or ordering requirements.

Local evidence is retained under `target/main-copy-sampling-0-baseline.*`,
`target/main-copy-sampling-analysis.json`, `target/disasm-geometry-copies.py`, and
`target/receiver-query-baseline.{exe,pdb}`. Baseline executable SHA-256:
`AFD5A941E97A6F2062A0D843560F52FD7F3875E044EB8FB8B682696CDD306680`.
