# Owned static M2 admission

The Build 155 live capture left main at 98.44% of one logical CPU, with only
10.73–13.10% on each CPU worker. M2 admission averaged 3.424 ms. That inclusive
stage contains much more than static spatial tests; these numbers do not assign
the whole cost to this change. See [the capture review](testing-build155-performance.md).

## Connected boundary

After native scene callbacks and WMO doodad visibility, runtime captures the
selected static candidates from compact retained metadata. The phase owns value
inputs: cached sphere/render bounds, distance class, native collector kind and
volumes, WMO shadow membership, light demand and current WMO visibility/opacity.
It borrows no model, placement, terrain coordinator, playback or GPU resource.

Workers apply the static shadow/distance/camera rejection gates before the
ordered placement traversal consumes their results. Dynamic units, vehicles,
attachments, moving-WMO doodads and effects continue to use their existing
ordered admission. The phase does not reorder callbacks, clocks, RNG, sound,
light publication, particles, ribbons or transparent ties.

This preserves the existing collector rules: 7BB9D0/7BA8F0 class/radius masks,
7BABC0's fade-start shadow cutoff, 7BCC00/7BC490 cascade collection and nearer
volume exclusion, plus exterior WMO doodad membership. Camera/portal culling and
ordinary opacity cannot remove independently admitted shadows or light owners.
The cached bounds use the same 7BDB10/7F9430 affine calculation. Invalid bounds
remain unvalidated until their original consuming predicate needs them; domain
errors return at that model's scene position, even if later jobs finish first.

## Scheduling and lifetime

Groups contain at most 64 scenery candidates. Sparse returned timing selects a
width targeting 100 microseconds, within that hard count limit; this is a work
estimate, not preemption or a deadline. Group results transfer once into a
retained consumption array. Each model then consumes its slot without acquiring
an executor result lock. Interleaved dynamic owners retain their original order.

Staging capacity, indices, cost hints and handles are budgeted before capture;
executor result cells charge their complete inline input/output arrays. Both
retained staging and executor allocations remain charged at peak. Group bodies
allocate no nested output streams. Reclamation restores jobs on success, error
and abandoned frames. A terminal stage services native readiness before retiring
the phase. Zero-static frames dispatch no spatial jobs.

Ordinary F10 scopes are `m2.static_admission.capture` on main and
`m2.static_admission.execute` per worker group. The execute owner is the first
placement, one-based; reason is group length. Existing CPU phase/consume/wait
links cover ownership and readiness. No per-model clock or informational log
was added. Calibration uses the common first-eight/one-in-64 sampling policy.

## Validation and limits

The external collector tests cover independent shadows, absent WMO membership,
hidden light owners, fade-start cutoff, camera exclusion, multi-group ordered
consumption with dynamic gaps, malformed input, abandoned suffixes, refusal and
reuse. The frozen serial-renderer comparison now includes static scenery beside
animated dynamic owners, including index relocation, moving cameras and abandoned
frames; it compares packets, palettes, effect state and the shared RNG stream.

Validation results are recorded in the [cutover status](cpu-cutover-status.md).
No live FPS gain or five-millisecond reduction has been established. Main still
captures compact inputs, visits final candidates, resolves dynamic admission and
receiver queries, and publishes final streams. Broad topology publication,
UI/main continuations, remaining resource graphs and full domain working-set
accounting remain requirements of the complete cutover.
