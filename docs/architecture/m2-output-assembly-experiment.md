# M2 output assembly experiment: separate phase rejected

On 2026-09-20 two implementations moved M2 final packet relocation and effect-stream
copying from main to the shared CPU workers. Both preserved tested output order,
but the added assembly join absorbed the main-thread saving. Neither implementation
is retained in production. Source was restored to the Build 166 checkpoint before
this report was committed; Build 166 remains installed.

## Experiment and validation

The coordinator assigned checked per-model prefixes in native admission order.
Workers received exclusive ranges in nine preadmitted final buffers, then relocated
mesh/shadow records and copied particle/ribbon streams into those ranges. The CPU
prototype provided non-cloneable owned ranges, retained accounting, and explicit
reclaim/abandon operations. There was no mutable world borrow on workers.

The first variant started assembly at normal M2 publication. The second attempted
to start it between independent ground-detail/WMO operations, while preserving the
later receiver callback boundary. Both retained animation, simulation, RNG and draw
order. The receiver oracle preserves the existing stock `821A20` / `831AF0` evidence.

The late variant passed formatting, full workspace Clippy with warnings denied,
and all 1,629 workspace tests (33 existing ignored; 100 suites). Tests included
five owned-range cases covering concurrent order, capacity refusal, incomplete
output, panic recovery, zero-sized values and budget lifetime. The moving-scene
M2 oracle compared packets, palettes, particles, ribbons, lighting, RNG and effect
state. The later early-overlap variant passed that oracle again with an additional
assertion that early preparation cannot publish receiver lights. It was not given
another full workspace run after the decision to reject it.

During validation, larger embedded worker state caused the existing application
startup test to overflow its normal debug thread stack. The failure reproduced
in isolation; heap-owned retained state fixed it without increasing stack limits.
Amortized final-buffer growth was retained to avoid full-stream reallocation for
each newly visible record. These prototype changes were removed with the experiment.

## Controlled full-frame results

Each variant used four alternating baseline/candidate runs of 4,096 frames, at
2560 x 1440, Ultra shadows, four CPU workers, 192 authored NPCs, and the same Soap
scene/camera sequence. Compilation and tests were stopped during measurements.
Stationary matching additionally required two resident tiles, zero admissions,
243 WMO draws, 482 far environment-shadow entries, 1,658 M2 draws and 23,864 bones.
These are offline comparisons, not live Brewfest FPS measurements.

| Variant / pair | Baseline matched stationary ms | Candidate ms | Reduction ms |
| --- | ---: | ---: | ---: |
| Late assembly 1 | 5.89255 | 5.84525 | 0.04730 |
| Late assembly 2 | 5.84875 | 5.78840 | 0.06035 |
| Early overlap 1 | 6.08225 | 5.82940 | 0.25285 |
| Early overlap 2 | 5.89120 | 5.82400 | 0.06720 |

The first early-overlap baseline was slower than the repeated baseline. Movement
results were mixed: early-overlap orbit medians changed from 5.15370/5.03760 ms to
5.05885/5.05420 ms; pointer-motion medians changed from 6.72845/6.68175 ms to
6.61740/6.77050 ms. Large isolated frames remained, including a 51.91 ms candidate
pointer frame. These runs do not establish a substantial or consistent movement
improvement, and do not approach the requested multi-millisecond reduction.

## Where the main-thread saving went

A separate early-overlap profiled pair contained 4,064 ordinary frames and 32 detail
frames per run. Ordinary accumulated scope time divided by frame count:

| CPU interval | Baseline ms/frame | Early-overlap ms/frame |
| --- | ---: | ---: |
| M2 admission | 1.90176 | 1.78560 |
| M2 publication | 0.71795 | 0.39307 |
| New prefix assignment, outside publication | none | 0.08479 |
| CPU phase reclaim pending | 0.000003 | 0.25664 |
| CPU result pending | 0.02217 | 0.04960 |
| New worker assembly, aggregate across workers | none | 0.72059 |

Publication plus the new prefix costs 0.47786 ms, saving about 0.24009 ms on main.
The additional reclaim-pending interval is about 0.25663 ms. Pending intervals
can include native event servicing and are not pure idle measurements. Aggregate
worker time is not critical-path wall time. Together with the unprofiled runs,
these observations explain why moving the copy into another phase did not yield
a useful full-frame improvement. The admission difference is not attributed to a
changed admission algorithm; none was implemented in this experiment.

## Retained evidence and next boundary

Ignored local evidence is retained under `target/`:

- `m2-assembly-prototype-source.zip` contains both the final prototype sources and
  the tracked patch against `fa2ea8cf`, plus a manifest of restored files.
- `m2-assembly-comparison-*` and `m2-assembly-early-comparison-*` contain raw frame
  CSVs, launcher scripts and analysis JSON for both variants.
- `m2-assembly-profile-*` and `m2-assembly-early-profile-*` contain the separate
  profile captures and analyses.
- The baseline executable SHA-256 is
  `CDC77269CC13A7E62D4219AB9ADCD1A3527671321A58673EBE30E161A32BDFF6`.
- Late candidate SHA-256:
  `7C1DAA05E069E231BC88A81C2FAE9BCB3D8A58A62AD5B2366538D2407BC5B071`.
- Early-overlap candidate SHA-256:
  `986AFCC9AFBA00BABF1C43E871C62DFD6E1F17BE8A7152CF2E1D8836BAC7CE6A`.

Do not repeat a separate final-copy phase as the M2 performance solution. The next
boundary should eliminate the copy through direct consumption of producer-owned
pages, or produce final records within the existing worker kernels. Preserve
compact stock ordering and receiver callbacks without adding a new join immediately
before their consumer. Main admission remains about 1.8-1.9 ms in this fixture and
still needs substantive distribution. This is a measured design constraint, not
completion of the [CPU cutover](cpu-cutover-status.md).
