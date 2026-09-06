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
subtracts each unsigned 32-bit authored frequency while following
`variation_next`; it does not normalize by the observed frequency sum. If the
chain ends before consuming the roll, the input/base sequence remains selected.

An exact requested variation is resolved before the weighted selector. Finding
that exact variation consumes no selection roll. Sequence timer construction
then consumes the next roll for its cycle count.

The frequency load and unsigned comparison at `0x00826E97` use the entire
32-bit word at sequence offset `0x10`. This is not a signed 16-bit value.
Zero or incomplete weights retain the input record when the roll is not
consumed; they do not imply a uniform distribution.

## Model Lua sequence requests

`SetSequence` (`0x009607E0`) and `SetSequenceTime` (`0x009608B0`) issue a new
request on every accepted call, including equal IDs and equal offsets.
The UI therefore publishes an ordered, instance-addressed command stream.
Glue drains it in call order; requests awaiting a decoded model remain FIFO
until that source is resident or the owning instance is replaced. Hidden
widgets retain CPU playback without allocating another visible GPU scene.
The stream is enabled only for the Glue owner that has a renderer consumer.

`0x00826350` resolves an absent animation through AnimationData fallbacks,
including reverse and held-endpoint modes. Direct authored presence wins
before fallback flags, even if external animation payload is unavailable.
The Model request supplies ordinal zero to `0x008260C0`: this is the lookup
head, not a search for a sequence whose variation metadata equals zero.
The request always consumes its weighted roll; an unavailable selected
payload leaves the current timer unchanged without consuming a cycle roll.
The root-bone `0xFFFFFFFF` request is a no-op at `0x00832840`.

`0x00826B00` creates a wrapping millisecond timer against the scene clock,
with a one-tick adjustment outside scene update. Explicit offsets change
that timer without replacing the model or clearing its effects. Sampling
at `0x0082F0F0` preserves signed scene differences and unsigned loop modulo,
including negative-offset behavior. Lua animation conversion preserves the
low 32 bits after the x87 integer conversion; the time argument follows
the SSE2 `_ftol2` path at `0x0088B9C0`.

Resident Glue requests now sample the owning clock when the command executes,
instead of reusing the model's last rendered tick. This matters after a hidden
interval: `Interface/GlueXML/AccountLogin.lua` calls `SetSequence(0)` in
`AccountLogin_OnShow`, so the next visible frame begins a fresh sequence.
Its `OnHide` calls `StopAllSFX(1.0)`; the sound engine cancels pending SFX as
well as active voices. Cinematic preparation retains immutable GPU resources
without activating or advancing the hidden login model. Entering a movie
retires any visible Glue effect frame, as other screens without a Model do.

The deterministic playback regression leaves the model unsampled for one
minute, issues the show request, and verifies time zero, no expired variation
callbacks, no accumulated sound events, and the first authored sound at its
new sequence-relative timestamp. Native deferred-load requests also carry a
scene timestamp at `0x00832AB0`; exact replay-time handling of that timestamp
remains a separate research gap in the FIFO loading path.

The automatic callback at `0x00831FC0` retains the portion of the frame after
a crossed boundary. `0x00832260` places looping callbacks at the last tick of
each authored cycle independently of the primary timer's total cycle count.
Scene-timer event windows follow `0x00830FB0`, preserving every crossed
occurrence, seek position, reverse mapping, and native timestamp order.
Held timers dispatch no keys, and nonlooping timers stop keys at their deadline.

Remaining gaps include event-position sampling at each individual callback tick,
event sound age, exact hidden
widget update/visibility clocks, and retaining hidden instances' GPU effects.
The existing world playback path still uses elapsed-time windows that collapse
multiple occurrences of one declaration and discards overdue variation time.
These Model changes do not establish parity for those callers or FrameXML
Model rendering. Scene-global track ownership also remains separate work.

## Automatic sequence blending

The Model timer path retains its outgoing primary timer during automatic
variation changes. At `0x00826C40`, the incoming sequence supplies the blend
duration. The envelope begins at the current scene tick, including when a
callback carries overdue time. A second callback preserves an existing
secondary while its contribution is strictly greater than 0.5; at exactly
0.5 it copies the outgoing primary instead. Explicit Model Lua requests
disable this blend, while unavailable requests and the root clear no-op
leave the current state intact.

`0x0082F0F0` samples the secondary independently, testing sequence flag `0x80`
for its completion clamp (`0x0082F592`) where the primary tests flag `0x1`.
The previous contribution is `smoothstep(remaining / duration)`. It becomes
zero at the deadline or when both sampled sequence/time pairs coincide.
The blend uses wrapping scene ticks and does not restart the previous timer.

Blending occurs before bone hierarchy composition and reaches continuous
material, light, ribbon, particle, and camera tracks through their shared
animation clock. `0x0082B0A0`, `0x0082AF40`, and `0x0082B340` interpolate vector,
fixed-point scalar, and float scalar results respectively. Step tracks return
before blending, global tracks bypass it, and discrete enable/selector tracks
retain the primary value (`0x0082B270`). Missing secondary keys contribute the
caller's track default.

`0x00828680` calls `0x00982460` for the rotation between complete sequence
samples. This is shortest-arc spherical interpolation, distinct from the
normalized linear key interpolation within a sequence (`0x00982630`). Its
near-collinear threshold is the pinned float at `0x00AA2E58`, exactly 2^-21;
below that sine magnitude it retains the primary quaternion.

This integration applies to the native Model timer path, including retained
posture owners for local players, remote players, and creatures. Equipment
secondary timer synchronization still requires integration.

## Unit primary posture ownership

The runtime's `UnitAnimationBehavior` owns unit playback independently of the
GPU placement. Replacing equipment or character materials borrows the
same primary timer. Scene update runs the primary completion callback before
visibility checks and passes its clock and expired event tails to visible
drawing, avoiding a second timer advance.

`UnitAnimationScene` retains these owners across material residency changes,
checking the full `WorldObjectIdentity` before reusing a GUID. Local-player
postures read Player_C's private stand state; remote players and creatures read
replicated `UNIT_FIELD_BYTES_1`. Character-selection handoff binds its world
owner before the first world GPU admission.

Remote and creature residency updates preserve every unchanged representation
when a neighbor enters or leaves. Each completed material/equipment generation
has a retained identity token; the renderer reuses matching bodies and their
child placements instead of preparing the whole visible set again. Changed
material/equipment generations for the same model rebuild GPU resources while
borrowing the same unit timer. A different model path starts a new playback owner.
A different world-object lifetime invalidates both the resident generation
and playback, even when its GUID and appearance match the retired object.

Unit body particles and ribbon trails also survive material replacement.
The renderer transfers their existing simulation allocations after all new
resources have prepared successfully, using the retained animation owner's
identity to require the same unit and model lifetime. New textures and particle
color replacements belong to the new material generation; live particle ages,
positions, pool phases, and ribbon history belong to the continuing model.

This follows the native texture mutation boundary: character atlas creation
at `0x004EFF10` calls `0x00825260` on the existing model. That setter updates
texture references, including ribbon `0x0097FAD0` and particle `0x00978C40`
references, without reconstructing either effect history. The Vulkan residency
test emits live particles and ribbon sections for local players, remote players,
and creatures, replaces their material resources, and verifies retained state
and continued aging. GUID reuse, model replacement, and world replacement
start empty. Disabling the transfer reproduces the test's material-change
failure. Effects across complete frame retirement still require retained
ownership; this transfer covers the unit body only.

World character publication also retains continuing equipped components,
including their GPU sources, playback/event clocks, particles, and ribbons.
New components prepare independently; retained placements detach only after
the complete replacement has prepared successfully. Publication restores
parent-before-child order while preserving their original source indices.
The same unit/model owner must remain alive, so GUID reuse cannot inherit gear.

The component boundary follows `0x004F2640`: helmet reuse at `0x004EF020`
compares the model path; shoulder reuse at `0x004EF710` requires both matching
paths when the equipped entry changes. An unchanged entry, including a
one-sided shoulder, survives an unrelated body material update. These native
reuse branches keep the old component's textures, flags, and attached visual
even if another item display names different values at the same model path.
Weapon entry or attachment-point changes create a new component through
`0x004EACD0`/`0x004EAA70`. An enchantment change follows `0x006D6BA0` and
`0x004EA8F0`, replacing the effect children while retaining their weapon;
an identical effect model path does not preserve that effect's old lifetime.

The equipment residency test covers local and remote characters, material
updates, same-path weapon replacement, same-path enchant replacement, helmet
and shoulder reuse, one changed shoulder, a one-sided shoulder, GUID reuse,
and failed preparation after a retention plan has formed. It checks real
Vulkan source identities, event clocks, live effect histories, and that
continuing components consume no initialization rolls. Disabling component
retention fails the material-update case. Glue previews, mount instances,
native component sequence initialization, and secondary-timer synchronization
remain outside this world-component lifetime change.

Changed stand state follows `0x0073F060`, including death entry through
`0x0073AF80`, submerged entry 201, and return from submerged through 127 or 224.
Ordinary sit/sleep/kneel/chair selection follows `0x0071E1F0`. Primary completion
uses the resolved clip's AnimationData behavior at `0x0073B510`: transitions
select their hold or exit sequence against the current posture. Missing poses
therefore retain their actual model fallback behavior.

Death completion preserves the outgoing variation for 1→6, 131→132, and
468→472. The first two requests enter CM2Model directly, where a missing corpse
can use the death clip's endpoint; 472 first passes through unit tier resolution.
Explicit Model variations follow `0x00832AB0`: an existing chain ordinal
consumes only the cycle-count roll and disables automatic variation changes;
a missing ordinal enters weighted selection. Chain position is independent of
the sequence's variation metadata ID. Exact unavailable ordinals remain
subject to payload admission rather than being replaced with another variant.
Other primary requests preserve an identical active animation and its random
state, following `0x00737EF0`.

`unit_stance_oracle.py` executes 2,144 original selection, entry, and completion
cases. Its hooks supply stand/model queries and intercept requests; they do
not replace the branch logic. It admits the ordinary resident unit without a
vehicle controller and does not prove spell/effect layers or the movement
water-height death probe. Runtime archive fixtures test transition chains,
interruption, fallback, random consumption, and timer retention. The
Vulkan scene test also verifies completion before culling and playback reuse
across placement replacement. A residency-to-Vulkan test covers remote and
creature posture changes, neighbor arrival/departure, material replacement,
and GUID reuse, checking retained source indices and random consumption.
The owner is not yet the complete Unit_C animation system: health/effect death
admission, layering, and offscreen event/effect delivery remain outstanding.

## Property-typed key storage and sampling

Key width follows the property's type independently of its interpolation
selector. The decoder now retains one typed key per timestamp. Camera tracks
use `M2SplineKey<Vec3>` or `M2SplineKey<f32>`; ordinary tracks retain their
single scalar, vector, or quaternion. Nested-channel decoding lives in the
asset animation `track` module.

| Property | Bytes per timestamp | Pinned validator / sampler |
| --- | ---: | --- |
| Ordinary vector | 12 | `0x008371C0` / `0x0082B0A0` |
| Bone compressed quaternion | 8 | `0x00836F80` / `0x00828680` |
| Texture-transform float quaternion | 16 | `0x00837010` / `0x0082AD50` |
| Camera position or target spline | 36 | `0x00837130` / `0x0082B460` |
| Camera roll spline | 12 | `0x008371C0` / `0x0082B8A0` |
| Fixed-point material scalar | 2 | `0x00836C00` / `0x0082AF40` |

Camera keys contain a value, incoming tangent/control value, and outgoing
tangent/control value even for step and linear interpolation. The camera
samplers advance by the full triplet, select its first value for step/linear
evaluation, and use the other two values for cubic interpolation. Ordinary
vector and scalar samplers retain their ordinary stride and use linear
interpolation for every nonzero selector.

Direct archive extraction confirms this in
`Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2`
(SHA-256 `875f665c96a6c97004aab94c09e194aa1b1c134352788c122ddfb2abb6331d65`).
Its linear roll track has two timestamps at byte `0x1481B0`: 0 and 66,667 ms.
The two triplets starting at `0x1481C0` are both
`(6.2831854820251465, 0, 0)`. The former decoder selected the first
key's incoming tangent as the second key. The angle-wrapping workaround that
masked that error has been removed: `0x0082B8A0` uses ordinary scalar linear
interpolation and performs no angle wrapping. The linear position track has
93 timestamps and 36-byte keys at `0x147450`; the third position value is
`(0, 0, -0.0025912390556186438)` at 11,600 ms. The corrected stride restores
these authored camera movements.

The Night Elf backdrop has the same triplet layout with step interpolation:
its camera position, target, and roll each have one all-zero key. Its M2 hash
is `77445315fb1d47eed3f20b962a5b6e1e00ad28325b1fdffc12d7d7eddc29b5f9`.
These original files have no rotation keys in their texture transforms.
The texture matrix builder at `0x0082D6F0` separately proves the float path:
it calls `0x0082AD50` on the rotation track at record offset `+0x14` before
`0x004C33C0` composes its matrix.

`0x00828680` expands each compressed quaternion component as unsigned 16-bit
times the float at `0x00A45560` (2/65535), minus one. Step sampling retains
that result directly. Non-step sampling calls `0x00982630`, which linearly
interpolates components without a hemisphere flip, then applies the polynomial
normalization at `0x00982570`. Matrix construction at `0x004C1C40` uses the
quaternion components directly. Compressed expansion, float-quaternion
decoding, key interpolation, and matrix composition now follow those rules
together. Nonzero selectors normalize even when the selected endpoints are
the same key. Neither decoding nor step sampling normalizes the stored value.

The normalizer uses exact float constants at `0x00AA2E5C..6C`, with bits
`3F82BE62`, `3F0852F8`, `3F758559`, `3F26F151`, and `3F6A4B55`. It starts with
`base - (length_squared - center) * slope`, applies another correction when
squared length is at most the final constant, and a third when it is at most
the preceding constant. Matrix composition accepts the resulting non-unit
components. Wider intermediates retain the native x87 rounding boundary at
stored floats; this is not a general bit-identical floating-point claim.

Ordinary particle emission-rate/lifespan tracks are linear for every nonzero
selector. Their preallocation bound therefore uses the largest stored value;
the previous synthetic cubic-overshoot calculation has been removed.

`tools/ghidra/animation_sampler_oracle.py` executes the original isolated
sampler, quaternion, and matrix instructions. It supplies only interval
indices/fractions and does not test timestamp lookup. Its numerical results
back regression cases for all four selectors, complete camera keys, an
authored full roll, ordinary vector stride, compressed bone rotations,
float texture rotations, and previous-sequence blending. Step and non-step
camera array-bound tests cover truncated triplet storage too.

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
