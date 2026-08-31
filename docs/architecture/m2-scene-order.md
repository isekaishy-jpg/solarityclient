# Build-12340 M2 scene ordering

This boundary was recovered directly from the fingerprinted `Wow.exe`
documented in `tools/ghidra/README.md` (SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`).
The addresses below refer only to that exact executable.

## Collection and pass selection

The world scene preparation at `0x00821A20` rebuilds one 0x44-byte element
array and three pass-index arrays for all visible M2 instances owned by the
scene. Independent placements therefore do not own independent transparent
ordering domains. `0x00823CB0` drains the selected pass and, for pass zero, a
second compatible-mesh grouping array.

Every layer in a material unit uses the pass selected by the unit's base
material at `material_index - material_layer`. A base blend value greater than
one enters pass one. Runtime element alpha below `0.99999` also promotes an
otherwise opaque unit into pass one. Such an authored non-blended layer uses
`SRC_ALPHA/ONE_MINUS_SRC_ALPHA` blending and stops writing depth; its shader
and authored alpha-test behavior do not change.

SKIN flags `0x1` and `0x2` replace the ordinary transformed section-center
distance with the near or far edge of its animated sorting sphere. The key is
the squared distance, with the transformed view-space Z sign retained on these
two branches.

## Transparent comparator

The common pass-one/pass-two comparator at `0x0081EF30` orders:

1. primary distance descending;
2. alternate-copy flag set first;
3. signed priority plane ascending;
4. secondary distance descending;
5. optional clip-state keys;
6. instance identity;
7. producer type and material layer; and
8. producer-specific mesh, ribbon, particle, or callback state.

The ordinary mesh path uses its section-distance key as both primary and
secondary distance. A separate scene/model/material branch can instead use the
model-origin key as primary and emit a second flagged copy; that branch remains
part of the unified scene-queue milestone rather than this mesh-only slice.

The stock arrays are sorted by the in-place heapsort at `0x0083DCF0`.
Complete equality is consequently not FIFO. Solarity uses an unstable sort and
does not add an insertion-order tie breaker.

## Current boundary

The runtime now shares transparent mesh order across every resident world M2
placement and performs runtime-alpha pipeline promotion. Pass-zero meshes
retain collection order until their compatible grouping comparator is carried
alongside ribbons, particles, and callbacks into one unified world scene queue.

The local player's body is another placement in this same M2 frame. It shares
the decoded M2/SKIN mesh and archive-backed hardcoded BLP identities while
owning its transform, animation clock, particle/ribbon histories, particle
color replacement, geoset visibility, and composed body atlas. Authoritative
movement updates only the placement matrix. A customization generation replaces
only the player source; it does not rebuild terrain M2s or duplicate same-path
HD replacement assets. Visible equipment now drives the body atlas, helmet and
armor geoset selection, and the body's cape texture slot before that source is
published into the frame. Head, shoulder, shield, melee-weapon, and ranged-item
M2s are separate player-owned sources in that same frame. Each child samples
its own animation and effects while its placement follows the body's current
animated attachment bone. The authoritative `UNIT_FIELD_BYTES_2` sheath byte
selects unarmed, melee-ready, or ranged-ready links; invalid states and absent
authored attachment points are errors rather than placement substitutes.

Pass-zero compatible grouping, attached item visuals/enchant effects, composite
character bounds, and the remaining callback producers still need to enter the
final unified scene queue.
