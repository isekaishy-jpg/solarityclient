# Worker-produced effect streams and bulk upload

The world renderer previously serialized every particle vertex, ribbon vertex
and particle index on main, after geometry workers had produced the complete
streams. The equipped fixture carries roughly 100,000 particle vertices per
frame. No shader transformation is needed at this boundary: the existing vertex
fields already describe the final PNC0T0/PCT0 payload.

## Contract

Particle and ribbon vertex types now declare C layout and derive `Pod`/`Zeroable`
through the existing bytemuck dependency. POD derivation rejects implicit padding;
separate compile-time assertions prove the 36-byte particle and 24-byte ribbon
strides. Focused vertex modules separate that ABI from simulation/geometry code.
Field values, accessors, floating-point equality, geometry order, particle RNG,
animation clocks and camera inputs retain their existing semantics.

On little-endian hosts, the world upload borrows the complete stream as bytes and
performs one checked copy per stream. It does not allocate, pre-copy into staging,
add jobs or introduce another dependency wait. Geometry workers already produce
the representation the shader consumes. Big-endian hosts retain explicit
little-endian encoding, following the existing palette-upload portability rule.
This is host byte-order handling, not a new gameplay fallback.

The existing slot fence, persistent mapping, range validation and VMA flush remain
authoritative. No additional unsafe operation is introduced. Errors still prevent
submission; invalid bulk ranges fail before writing their stream.

## Evidence and limitations

A hidden instruction sample used the preserved Build 170-source benchmark,
SHA-256 `8EE83E428963FE5DFA4011ACE9B29D5891D29789AEDA623E6BF59F05B51F4318`,
with the existing 300-equipped-NPC fixture. Sampling covered seconds 35-55 after
launch. It collected 6,388 samples; median suspension was 25.6 microseconds and
p95 210.1 microseconds. Only the owned diagnostic child was sampled.

There were 238 samples in `WorldFrameSlot::write`, 98 in particle vertex
`to_bytes`, and 334 in `memmove`, predominantly called by upload/recording code.
The sample also spent 41.59% in the native presentation entry and 16.47% in the
native message wait entry. These are instruction observations, not exclusive
CPU duration measurements. Nearest exported NVIDIA symbols do not establish
internal driver function identities.

The sampled run had substantially higher presentation time than the earlier
controlled fixture. Its frame timings are not a before/after comparison, nor
proof that encoding accounts for the entire difference. The main-admission
loop and queue setup were not one dominant instruction concentration in this
sample; that does not invalidate their costs in the separate live capture.

Local evidence: `target/admission-instruction-sampling-0-baseline.*` and
`target/admission-instruction-analysis.txt`.

## Verification status

Four new ABI tests compare bulk output with the existing explicit serializer
across empty, short and large streams, packed color lanes and arbitrary float
bits including signed zero, infinity and NaN. They also verify guard ranges and
out-of-bounds refusal. Workspace Clippy passes; all 1,646 tests pass with zero
failures and 33 existing ignored tests across 101 suites. Formatting and optimized
compilation pass. The measured candidate SHA-256 is
`07F4BD9FEFC584A4909D5B41CE137ECB3A0D31DF59BB9D69AE8AA72FFF5ED995`.

## Controlled comparisons

Eight hidden runs complete 16,384 frames: four alternating equipped comparisons,
two unarmed controls and a separate equipped profile pair. Each run has 512
frames each of streaming, stationary, orbit and pointer camera phases. No
compiler or GPU test overlaps these measurements. Only owned diagnostic clients
are launched; an existing user client prevents launch.

The equipped fixture uses Soap on map 1 at (1515.34, -4417.27, 18.0499), 300 NPCs
with display 6882, main hand 50732 and off hand 18805, four CPU workers/capacity
256, two network workers, 2560 x 1440 Ultra and GTX 1070. It excludes live
networking, audio, movement solver and overlays. The unarmed control uses 192
NPCs. These are synthetic comparisons, not the user's live Brewfest scene.

The common stationary tuple has two resident tiles, zero admissions, 243 WMO
draws, 482 far-shadow draws, 3,450 M2 draws and 41,516 bone transforms. Selection
maximizes the minimum representation across runs independently of timings.

| Matched stationary result | Baseline 0 | Candidate 1 | Baseline 2 | Candidate 3 |
| --- | ---: | ---: | ---: | ---: |
| Frames | 308 | 316 | 309 | 349 |
| Median ms | 27.1995 | 21.6812 | 26.7718 | 23.6535 |
| p95 ms | 30.3466 | 26.7738 | 31.3259 | 26.2533 |
| Maximum ms | 62.6885 | 76.0100 | 48.6643 | 30.0969 |
| Median particle vertices | 107,038 | 107,544 | 106,992 | 107,536 |

Matched median reductions are 5.5183 and 3.1183 ms, with slightly more candidate
particles. Orbit and pointer medians also fall, but tails are mixed. The first
candidate streaming maximum is 429.8516 ms versus baseline 108.4798 ms; the
repeat is 92.5035 versus 110.5689 ms. This does not establish a hitch fix.

The unarmed matched stationary median changes from 12.5564 to 12.2374 ms
(0.3190 ms lower), with approximately 8,000 particle vertices. Its matched tuple
has 1,658 M2 draws and 23,864 bone transforms, with the same terrain/WMO counts.

### Attribution and presentation limits

Each separate profile has 2,032 ordinary frames and 16 detail/GPU samples.
These all-phase averages are not the matched stationary medians above.

| Profile mean ms/frame | Baseline | Candidate |
| --- | ---: | ---: |
| Slot wait and upload | 2.2089 | 1.6182 |
| Command recording | 1.9495 | 2.2255 |
| Queue present | 10.6271 | 11.1493 |
| Renderer wall time | 15.2088 | 15.4661 |
| Main M2 admission, both sites | 3.7500 | 4.5708 |
| Main publication | 0.2832 | 0.3322 |
| Coordinator wait | 2.3801 | 3.0267 |
| Frame wall time | 23.6891 | 25.8048 |
| GPU, 16 samples | 9.1012 | 9.7879 |

Upload falls 0.5907 ms; renderer charged thread cycles fall from 17,066,190 to
15,179,662 per ordinary frame (about 11.1%). Cycles are not converted to elapsed
milliseconds. Total profiled frame time is worse, including higher unrelated
main admission and present waits. Therefore the repeat unprofiled improvements
do not establish a 3-5 ms CPU encoding saving or a universal whole-frame gain.

Current baseline presentation is substantially slower than in the earlier
Build 170 comparisons. Compare only the alternating runs under these current
conditions; do not compare candidate totals with those historical baselines.
The repeat upload/cycle evidence supports removing redundant serialization,
while presentation behavior and live performance remain unresolved.

Local evidence: `target/effect-stream-upload-comparison-analysis.json`,
`target/effect-stream-equipped-profile-analysis.json`,
`target/effect-stream-upload-profile-phases.txt` and the corresponding
`target/effect-stream-*` run logs. Numbered packaging is recorded separately.

This boundary does not complete main-thread admission distribution or the
[remaining cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover).
