# Rejected M2 borrowed-output experiment

Four storage/consumption variants were evaluated on 2026-09-20 against Build 166.
None established a useful whole-frame gain. All prototype source and tests were
removed. Production code and the installed Testing client retain Build 166
behavior; the remaining main-thread M2 cutover stays open.

## Boundary tested

Workers retained initialized mesh/shadow packets and particle/ribbon streams.
Main published compact palette offsets, receiver indices, scene order and stream
prefixes instead of concatenating full output arrays. Rendering consumed those
pages without another worker phase or completion join. Animation, callbacks,
RNG, visibility, shadows, receiver queries and transparent order stayed unchanged.
This tested an alternative to the [rejected assembly phase](m2-output-assembly-experiment.md).
It does not establish new stock behavior from the modern Classic executable.

The variants used, in order: per-packet callbacks with mapped iterators; direct
numeric iterator jumps; borrowed checked frame state; and concrete typed page
arrays replacing packet callbacks. Typed pages transferred whole budgeted buffers
back to their original jobs before next-frame generation/demand reuse. Effect
streams remained in producer jobs, with page-wise access. Duplicate bulk storage
was removed, but repeated renderer consumers became more expensive.

## Controlled comparison

The hidden offline fixture used Soap, 192 authored NPCs, the same Orgrimmar terrain,
2560x1440/Ultra, four CPU workers and identical camera replay. Each variant had two
alternating baseline/candidate pairs, each run covering 4,096 frames across four
equal streaming/stationary/orbit/pointer phases. Profiling was disabled. Compilers
and tests did not run concurrently. The fixture excludes live networking,
movement solving, audio and overlays; it does not establish desktop FPS.

Matched stationary frames have two resident tiles, no tile admission, 243 WMO
draws, 482 far shadow casters, 1,658 M2 draws and 23,864 bone transforms.

| Variant | Pair 1 baseline -> candidate | Pair 2 baseline -> candidate |
| --- | --- | --- |
| Mapped iterator | 5.871 -> 23.306 ms | 5.825 -> 23.125 ms |
| Numeric cursor | 5.900 -> 6.276 ms | 5.904 -> 6.117 ms |
| Borrowed frame state | 5.835 -> 5.810 ms | 5.899 -> 5.828 ms |
| Typed pages | 5.849 -> 5.994 ms | 5.915 -> 5.885 ms |

Typed-page orbit medians were 5.032 -> 5.082 ms and 5.076 -> 5.051 ms.
Pointer medians were 6.799 -> 6.859 ms and 6.765 -> 6.781 ms. Long frames remained,
including a 254 ms first streaming frame in one candidate run. These mixed results
do not justify shipping this boundary as a performance fix.

## Causal findings

Mapped iterators initially caused quadratic work: repeated batching suffix scans
used skip, and the pinned Rust Map implementation resolved skipped elements through
default nth/advance_by. Main recording rose from 1.014 to 15.731 ms. Direct numeric
cursor jumps fixed that defect. A regression fixture bounded a 4,095-element prefix
jump to at most two resolutions. No affected executable was installed.

Separate instrumented runs recorded these ordinary-frame averages over 4,064
frames per executable, excluding the first 32:

| Scope | Baseline | Typed pages |
| --- | ---: | ---: |
| Main M2 admission | 1.904 ms | 1.938 ms |
| Main M2 publication | 0.729 ms | 0.446 ms |
| Receiver completion | 0.125 ms | 0.076 ms |
| Renderer resource admission | 0.048 ms | 0.119 ms |
| Slot wait and upload | 0.603 ms | 0.691 ms |
| Main command recording | 1.010 ms | 1.078 ms |
| Whole world CPU rendering | 1.935 ms | 2.188 ms |

World CPU rendering includes the listed renderer phases; do not add them again.
Shadow capture is included in recording (0.197 -> 0.243 ms). Aggregate worker shadow
recording stayed near 0.75 ms. GPU timing had only 32 samples per run and does not
establish a GPU improvement from this CPU change.

Publication saved about 0.283 ms while world CPU rendering added about 0.253 ms.
Removing per-packet callbacks did not resolve the trade. The profile identifies
which consumers grew, not a particular cache miss or instruction responsible.
A future output redesign must provide efficient final renderer records as well
as eliminating copies. Another equivalent page adapter or late copy phase should
not be repeated without new evidence. Main admission remains around 1.9 ms here;
distributing its substantial kernels remains a priority.

## Validation and retained evidence

The first and numeric-cursor variants passed workspace formatting, Clippy and
1,627 tests (33 ignored, 100 suites). The state and typed-page variants passed
all-target/all-feature Clippy and optimized benchmark builds. They were rejected
before another full test run. The final archive includes an additional unrun
allocation-transfer assertion added after compilation; it is not in the benchmark
executable or a claimed passing test.

All 36 modified crate files were restored to 187d5a5d and all 18 prototype additions
were removed. The final source has no production changes. Ignored target artifacts:

- m2-pages-*, m2-pages-indexed-*, m2-pages-state-*, m2-pages-typed-* contain executables,
  exact PDBs, comparison CSVs, profiles and analysis scripts.
- m2-pages-typed-rejected-source.zip, m2-pages-typed-rejected.patch and the source
  manifest preserve the prototype outside production.
- Baseline m2-assembly-baseline.exe SHA-256:
  CDC77269CC13A7E62D4219AB9ADCD1A3527671321A58673EBE30E161A32BDFF6.
- Final candidate m2-pages-typed-candidate.exe SHA-256:
  FB35694A25C1F6B2B609CD4A0336BFC2A9D592A09F424130A1592018CBBEE91B.
