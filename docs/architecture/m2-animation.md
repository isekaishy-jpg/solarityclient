# Build-12340 M2 animation selection

This boundary was recovered from the fingerprinted `Wow.exe` documented in
`tools/ghidra/README.md` (SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`).
The addresses below refer only to that exact executable.

## Random stream

Build 12340 links the Visual C++ 2005 `_rand` implementation at `0x0088B867`.
The CRT per-thread-data initializer at `0x0040DE57` writes `1` to the
`_holdrand` field, and the executable has no linked `_srand` symbol. Each call
therefore advances one client thread's state as:

```text
state = state * 214013 + 2531011       (wrapping u32)
result = (state >> 16) & 0x7fff
```

Solarity retains one `CrtRand` at the client-thread composition root. M2
instances consume that shared stream in placement order; model paths do not
seed independent generators.

## Sequence lookup and variation selection

`CM2Model` sequence setup at `0x00832AB0` distinguishes two cases:

1. A model with no animation lookup table scans its sequence records for the
   requested AnimationData ID.
2. A present lookup table uses the authored starting bucket and quadratic
   probing. A miss is authoritative and does not fall back to a record scan.

The weighted selector at `0x00826E60` consumes the raw 15-bit CRT result. It
subtracts each non-negative authored frequency while following
`variation_next`; it does not normalize by the observed frequency sum. If the
chain ends before consuming the roll, the input/base sequence remains selected.

An exact requested variation is resolved before the weighted selector. Finding
that exact variation consumes no selection roll. Sequence timer construction
then consumes the next roll for its cycle count.

## Key-bone lookup

The semantic key-bone table at header offset `0x34` contains signed 16-bit bone
indices. Exactly `-1` denotes an absent role; all other negative values and
nonnegative indices beyond the skeleton are invalid. Solarity decodes this
table directly and never widens it through an unsigned intermediate.

Runtime consumers resolve a semantic role through the authored table only.
They do not scan the bone array's `key_bone_id` fields to repair an absent or
malformed lookup.

## Owned header lookups

The fixed-width lookup tables at `0x68` and `0x78` through `0x98` are decoded
directly. Malformed headers cannot be repaired into empty tables, and larger HD
replacements do not incur duplicate lookup allocations.

Bone and texture lookups require an existing record. Replaceable-texture,
texture-weight, and texture-transform lookups preserve only `0xFFFF` as an
absent entry. Texture-coordinate selectors remain signed because negative
values participate in the stock environment-coordinate branch; the format
boundary does not reinterpret them as ordinary UV-set numbers.

An omitted optional weight or transform lookup makes the corresponding SKIN
combo word an identity selector even when that word is zero. Item M2s also use
one texture-weight selector for the whole material batch, not one entry per
texture stage. Transform combos remain stage-indexed. These stock encodings are
validated directly rather than padded into synthetic lookup entries.

## Cycle count and ownership

Sequence timer construction at `0x00826B00` calculates the total number of
cycles with integer scaling:

```text
cycles = minimum + ((rand15 * (maximum - minimum)) >> 15)
cycles = max(cycles, 1)
```

The upper bound is exclusive. This is not modulo selection, and the result is
the total play count rather than a number of additional repeats.

Mutable sequence, timer, and variation state belongs to each placed M2. Parsed
M2/SKIN data, textures, mesh buffers, pipelines, and descriptor sets remain
shared by asset identity. This keeps independent doodad variation behavior
without duplicating large HD replacement assets or GPU resources.

The asset crate is the sole production decoder and allocation owner for these
arrays. An HD-sized model is neither cloned nor decoded into a redundant
animation graph.
