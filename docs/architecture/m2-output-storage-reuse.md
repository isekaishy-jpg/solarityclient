# M2 output storage reuse

The Build 160 live run ended at `2026-09-20T08:45:25.516774Z` with
`cpu storage needs 27216 bytes with 23432 available in Frame`. This was a
controlled event-loop failure at the configured 128 MiB CPU Frame budget.
It does not establish a camera panic or a worker timeout. The log does not
identify the individual reservation that failed.

## Retention defect

Geometry admission previously selected its retained job by traversal ordinal.
Clearing output lengths retained every buffer's capacity. As camera admission
and order changed, a particle-heavy source could occupy successive ordinals,
leaving its large buffers behind for unrelated models. Each ordinal accumulated
the component-wise largest outputs of models that had passed through it.

The earlier Build 159 F10 audit recorded Frame storage reaching 113,913,528
bytes, including 109,205,696 bytes of Result storage. That supports examining
retained output capacity, but is not an allocation trace of the later exit.

## Ownership rule

The geometry reuse module now indexes only the preceding admitted frame's jobs
by immutable source generation and visible/shadow demand. Intrusive slot lists
allow repeated placements to each take one matching allocation. A weak source
reference preserves allocation identity without pinning the source payload.
Unmatched prior jobs release their buffers during phase reclamation.

The warmed lookup costs one linear indexing pass and expected constant time per
admission. Ordered submission/publication, animation and effect clocks, RNG,
effect-state return, and gameplay visibility rules are unchanged. The Frame
budget remains 128 MiB. New demands still require real capacity; this does not
make an arbitrarily large admitted scene fit that budget.

Detailed profiling adds `m2.geometry.retained_frame_bytes` and
`m2.geometry.retained_result_bytes` after reclamation. These measure the global
Frame ledger and its Result category at that point, not geometry-exclusive
allocations. Their ledger snapshot runs only with detailed profiling enabled.

## Verification scope

The rotation regression moves one large emitter among 32 model ordinals for
256 frames under a 48 KiB budget. Matching reuse retains 33,264 bytes throughout;
ordinal reuse would exceed the budget when the emitter moved to another slot.
Additional tests cover demand/generation retirement and repeated placements
consuming distinct slots. Existing moving geometry parity tests exercise
worker/serial outputs and effect-state return.

Formatting and full workspace Clippy (all targets/features, `-D warnings`) pass.
Full workspace tests pass: 1,603 passed, zero failed, 33 existing ignored tests,
including the moving worker/serial geometry comparison and the new regressions.

The hidden baseline replay covers 896 frames across streaming, stationary,
orbit, pointer-look, travel and settling phases at the authored Orgrimmar fixture.
It completed without the reported budget refusal, so it does not reproduce the
user's exact scene. Hidden replay timing is not a matched live FPS measurement.
The same 896-frame replay with the fix also completed successfully. Its seven
detail samples reached 12,580,168 Frame bytes and 3,790,872 Result bytes, with no
dropped profiling samples or capacity overflows. These sampled maxima are not
whole-run allocation high-water marks and cannot be compared directly with the
different live scene above. Capture: `1789895057856-1`, under the ignored
`target/geometry-storage-after-profile/Profiles` diagnostic directory.
The user reported partial camera improvement in Build 160; the remaining motion
hitches and exact live crash reproduction remain separate validation limits.
