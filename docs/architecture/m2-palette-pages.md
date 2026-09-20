# Worker-owned M2 palette pages

M2 geometry workers already produce each model's render palette. Ordered main
publication previously concatenated those palettes into another frame vector;
world upload then serialized every matrix into mapped GPU memory. Instruction
sampling located both transfers; see [the evidence](m2-main-copy-evidence.md).

## Ownership and ordering

The completed `GeometryJob` now retains its palette until synchronous renderer
upload ends. Publication advances a checked logical bone count and marks each
palette selected by the existing visible-or-shadow rule. Draw relocation still
uses the same ordered prefix, including offscreen shadow-only casters. Jobs with
no selected palette expose an empty page. No page index vector is allocated.

After the entire phase is reclaimed in submission order, `M2VisibleFrame`
borrows the job bank through rendering's `M2BonePaletteSource` interface. This
borrow prevents frame preparation and palette reuse until the renderer returns.
Only the existing particle/ribbon state returns to placements at reclamation;
that operation does not alter the retained palette. Abandoned or failed frames
still reclaim every job and expose no visible frame. Reuse resets publication
state, and unmatched previous-generation jobs retire as before.

Rendering accepts contiguous palettes and borrowed page sources through the same
world-frame boundary. It copies world pages followed by sky palettes into the
available frame slot. The slot's existing fence and mapped-memory flush remain
in force; no CPU palette borrow escapes to asynchronous GPU work. Checked byte
ranges and the declared total reject mismatched sources before submission.

The GPU wire format remains 16 little-endian column values per matrix. On the
supported little-endian targets, glam's existing bytemuck feature supplies a
checked POD slice view and upload copies complete pages. Big-endian encoding
retains explicit scalar conversion. No new package dependency was introduced.
An empty descriptor still receives its previous zero matrix.

The large world-frame resource module now separates mapped upload from resource
lifetime code in a folder module. Palette source and byte encoding also have
separate children. The frozen serial test oracle retains its own flat vector
under `cfg(test)`; production has no concatenated M2 frame palette.

## Scope

This connects one required worker-output ownership boundary. It does not move
ordered dynamic admission, callbacks or spatial queries to workers. Draw records
and particle/ribbon output still have main publication and upload costs. Broad
character batching and complete nested working-set accounting remain open. The
camera/input order, simulation clocks, RNG, culling, shadows and bone math are
unchanged by this representation change.

## Validation

`cargo fmt --all -- --check`, workspace Clippy with all targets/features and
warnings denied, and `cargo test --workspace --all-features` passed. The full
suite reports 1,623 passed, zero failed and 33 existing ignored across 99 suites.

New upload tests compare disjoint pages plus sky against the previous scalar
wire encoding, including NaN/infinity bit patterns, empty pages, empty descriptors,
untouched trailing capacity and invalid lengths/ranges. The existing frozen
serial-versus-worker integration test compares every matrix and draw while the
camera moves, placements change membership, lights change, shadows remain
visible offscreen and pending preparation is abandoned. Its serial storage is
confined to the test oracle; comparison materializes pages only in the test.

## Optimized comparison

Four unprofiled hidden runs alternated baseline/candidate/baseline/candidate,
4,096 frames each (1,024 streaming, stationary, orbit and pointer frames).
The baseline is `ee3d3b43`; candidate includes only this palette representation
change and its required renderer wiring. No compiler or GPU tests overlapped.
The same Soap fixture, 192 authored NPCs, camera phases, 2560 x 1440, Ultra
shadows, GPU 0, VSync off, four CPU workers and two network workers were used.
This offline fixture does not reproduce live network, movement-solver or audio
work, nor the exact Brewfest population.

The stationary comparison selects two resident tiles, no new admission, 243 WMO
draws, 482 far-shadow casters, 1,658 M2 draws and 23,864 bones:

| Run | Matched frames | Median ms | p95 ms |
| --- | ---: | ---: | ---: |
| Baseline 0 | 407 | 6.8960 | 7.8784 |
| Candidate 1 | 361 | 6.5178 | 7.4343 |
| Baseline 2 | 388 | 6.9849 | 8.0542 |
| Candidate 3 | 399 | 6.4519 | 7.4419 |

Paired matched medians improve by 0.3782 and 0.5330 ms (5.5% and 7.6%).
Whole-phase orbit medians are 6.059/5.956 ms for baseline and 5.624/5.698 ms
for candidate. Pointer medians are 7.739/7.811 ms versus 7.366/7.435 ms.
These results support this ownership change, not the requested multi-ms overall
improvement or a live FPS claim. Other mutable scene work, including particles,
continues to evolve during each run even where the selected draw counts match.

Long-frame maxima are not a guarantee of hitch-free behavior. The first
candidate has a 254.694 ms initial streaming frame and a 45.430 ms orbit outlier;
the second candidate's maxima are 56.309 ms streaming and 12.457 ms orbit.
Baseline maxima reach 61.908 ms streaming and 34.076 ms pointer. The unprofiled
runs do not identify those outliers' instruction-level causes. Neither candidate
shows the repeated multi-second stalls of the rejected receiver prototype.

Local artifacts: `target/palette-pages-comparison-*` and the analysis JSON.
Candidate executable SHA-256:
`7332569DBEDB22E319BEC1571CB7DA45B72BC8498A946A3402A1D9414D5AFF7F`.
