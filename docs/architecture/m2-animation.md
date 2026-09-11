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
Scene-timer event windows map crossed occurrences through `0x00830FB0`'s
seek/reverse arithmetic. The complete model scan can replace those candidates
before dispatch, as described under the shared active-bone scan below.
Held timers dispatch no keys, and nonlooping timers stop keys at their deadline.

Remaining gaps in the earlier single-primary path include queued event-position
sampling across timer changes, event sound age, exact hidden
widget update/visibility clocks, and retaining hidden instances' GPU effects.
The existing world playback path still uses elapsed-time windows that collapse
multiple occurrences of one declaration and discards overdue variation time.
These Model changes do not establish parity for those callers or FrameXML
Model rendering.

## Shared active-bone callback scan

Units and models retaining explicit bone slots use `832260`'s shared scan cursor
and newest-activation-first linked-list order. Reselecting an active slot keeps
its position; clearing it removes it from the callback scan. All slots see the
same previous tick on each scan. Independent per-slot catch-up loops can produce
different callback counts, order and random draws on a long frame.

Completion eligibility is computed before each slot's event scan. `82E790` can
therefore replace an earlier event queued by that same slot. An overdue terminal
deadline can move the shared cursor backwards, and later slots still compare
their deadlines with that bound. `831FC0` checks the queued sequence and start
against the current slot before calling its owner, then checks again before
automatic variation selection. A callback can replace or clear another slot.

`830FB0` admits every event for bone zero. For another active bone, ancestry
starts at the event bone's parent, excluding the event's own bone. A parentless
event bypasses that traversal and can occur once per eligible active slot.
Global event tracks select channel zero but still use each slot's timer mapping.
Queued event poses are sampled at the current scene time before tied completion
callbacks execute. Reconstructing events from broad time intervals afterward
would replay candidates that the native queue discarded.

`826C40` marks a zero-span or already-ended terminal request finished on
activation. Selecting the same finished sequence preserves its existing blend
slot. A bone's first primary selection has no inherited outgoing primary.
`832840` clears an upper primary immediately, preserving an existing blend above
half weight or copying the outgoing primary into a 150 ms fade. Root clears
remain excluded. Model pause shifts all explicit bone timers while global tracks
keep their construction origin.

Two pinned executable captures validate the production scanner and playback:

- `model_bone_callbacks_oracle.py`: 576 complete callback scans with different
  activation orders, terminal/looping sequences, speeds, seeks, event ancestors,
  and scene intervals. Fixture SHA-256:
  `f51b7db489cadcce9a944a6b2a8c8d1490e6d9d505521f72a52d27adfcc70efe`.
- `model_bone_playback_oracle.py`: 288 model updates with automatic variations,
  cross-slot replacement, clearing and RNG consumed by authored callbacks.
  Callback order, final timers, retained blends and the shared random state match.
  Fixture SHA-256:
  `0f1115ec19a526685d2160e17c71ac4c314eeddff4f5c19f63f5669ca72eb04f`.

Both captures execute original activation, scan, queue and completion code.
They supply application callbacks, CRT rand and an already-sampled bone palette;
they do not validate native bone-transform calculation or GPU submission.
The default effect-load capture also records its previous-sequence index, including
the held-sequence case that must not invent a blend.

Unit body models now invoke their environmental effect consumers synchronously
inside this scan. CEffect default-sequence construction therefore consumes its CRT
draws before the next callback or unit model update. Event poses retain the scan's
snapshot, and the later geometry pass does not replay those events.

Unit pending selections, body yaw and opacity advance first. Ground placement and
vehicle targets then sample current timers without executing callbacks. The scene
pass follows attached model subtrees, refreshing passenger and rider transforms
after a parent timer changes. Requested CEffect anchors update independently of
camera visibility. Disabled attachment channels suppress child callbacks while
their timers age; camera culling does not substitute for that channel.

`model_scene_order_oracle.py` executes `81C9C0` and `832450` for 480 supplied
root/child lists. The runtime's cached traversal matches their complete subtree
order, including siblings whose placement records are separated by other roots.
Fixture SHA-256:
`db90f78f8aae648d71d390a50d8efa6b55a48d03f691f4bd7a711fe82cd4eefd`.
The capture supplies context housekeeping and records entry to the inner model
scan; the preceding captures validate that scanner. It does not recover the
producer of root registration or sibling attachment order. The runtime preserves
its existing order within those lists, so whole-world RNG equivalence remains
unproven.

A renderer regression observes the next unit's unconsumed event interval during an authored
effect callback, verifies current moved world positions and same-frame CEffect
construction, and retains delivery for offscreen units. Mount models now use the
shared authored scan before their riders, including camera-culled mounts. Native
`73D5D0` registers the same `734A40` unit adapter on the mount. Its event position
comes from the emitting model, while the `6F9260` breath factory resolves attachment
17 (fallback 19) through the unit body at `+B4`. The renderer keeps those owners
separate: the mount supplies the captured event point, and the consumer receives
the current body model and its saddle transform. Bone queries sample current
timers without advancing a second callback scan.

Mount construction now uses the shared native default-sequence path rather than
the legacy elapsed-time playback owner. Resolved movement changes create native
primary timers and blends; unchanged forward ID/rate requests preserve the
selected variation, event cursor and CRT stream. Constructor fallback modes are replaced
when movement explicitly selects the same clip in forward mode. Local players,
remote players and creatures share that selection path. This does not complete
the mount's higher-level Unit_C animation policy.

`unit_model_effect_binding_oracle.py` executes `734A40`, `732650` and `6F9260`
for 48 body/mount emitter, attachment, breath-state and unit-lifetime inputs.
All 24 live cases query the body regardless of the mount's attachment availability;
missing units construct no effect. GUID lookup, finite-coordinate validation,
model readiness and attachment queries, allocation and model construction are
provider boundaries. The probe establishes factory model selection, not the
providers' bone calculation, constructor random draws or CEffect rendering.
A GPU regression supplies opposite breath attachments on mount and body, verifies
mount-before-rider delivery and distinct event positions, and checks same-frame
replacement effects on the body's attachment while both meshes are offscreen.

The ordinary mount owner completion (`73BFF0` -> `73B510`) now runs in that shared
scan. Mount timers are bound to the retained unit owner, so each captured movement
request resolves against the current body behavior and commits the mount before
the body/upper slots. Jump and landing requests survive body model replacement;
dismount consumes earlier mounted requests before detaching the binding. Without
a new request, takeoff retains its timer until completion selects the jump loop.
A later airborne reevaluation reads the body's behavior (`724200`), so it can
interrupt a mount takeoff while the body still plays rider pose 91.

Mount completion resumes at the current scene tick without the body callback's
overdue offset. Direct corpse transitions retain the emitting model/key's
variation; Unit-level corpse requests test the body's death family before taking
the mount root's variation ordinal. Mount requests do not set the body's landing
bit. Normal behavior-39 completion clears it; interruption checks raw IDs 39/187.
Resolved mounted requests use `71D6B0` for mount admission and `71D800` for upper
admission; `71D550`/`71DDE0` can force the body even under a mount-only callback
mask. Death resolution uses the mount before the body's own model fallback.
Finished timers and expired authored ranges are normalized to a missing current
mount ID by `7173F0`, allowing an identical animation to be submitted again.

`unit_mount_owner_oracle.py` captures 504 original-instruction dispatches with
resident sequence providers and inactive combat/passenger providers. Runtime
tests compare 56 ordinary live/dead dispatches, including independent body and
upper bones. Further tests cover packet order, model replacement, corpse ordinal
selection, terminal reissue and jump/landing progression on all three offscreen
renderer paths with the shared CRT stream. The probe does not execute model
commit consumers or prove the supplied providers.

The `73C090`/`73C140` vehicle-controlled completion branches now consume seated
passenger ownership, including shared keys, interruption and replay with the
completion overrun. See [vehicle-owned ride clips](vehicle-presentation.md#vehicle-owned-ride-clips)
for original-instruction comparisons and the separate model/renderer checks.
Entry/exit action ownership, animation redirect, flight-takeoff/action latches,
weapon readiness conversion and full action-priority providers remain open.
Other generic models
retain the earlier single-primary path. Sound callback age,
the complete scene registration lifecycle, Unit_C spell/action providers, equipment
synchronization and remaining vehicle-control animation providers remain open.

## Per-model global tracks

The constructor at `0x00834810` stores the scene tick in `CM2Model +0x74`.
`0x0082F0F0` samples each global sequence using the wrapping unsigned elapsed
tick modulo its authored duration. This origin is independent of the primary
sequence and the particle-update timestamp at `+0x8C`; primary seeks, changes,
and pauses do not restart or stop global tracks.

`M2Playback` now owns that construction tick for Glue, units, equipment,
game objects, and placed models. CPU unit admission receives the current scene
tick, and material/GPU rebuilds retain the same playback owner. A replacement
model or unit lifetime receives a fresh origin. Track sampling computes the
integer remainder before converting to float, preserving the phase when an
elapsed tick is too large for exact float representation.

`model_effect_clock_oracle.py --global-output` executes the original constructor
and global phase instructions. Its 240 committed cases cover nonzero creation,
scene wrap, zero durations, and large unsigned elapsed ticks. Decoded particle
track and runtime ownership regressions cover sampling, seeks, pauses, and
material replacement. The scene-facing runtime API still supplies float
milliseconds; quantization at that earlier boundary remains a separate clock
precision gap.

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
reuse branches keep the old component's textures and attached visual
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
and native component sequence initialization remain outside this
world-component lifetime change.

Equipment models retain independent sequence playback and authored orientation.
The build-12340 component factory `0x004EAA70` passes the model path, replacement
texture, item visual, and particle-color ID into the new child. It never reads
`ItemDisplayInfo.flags` at record offset `0x28`; `0x004EA9E0` handles the particle
colors at offset `0x60`. The generic attachment operation `0x00831630` links the
parent and attachment ID without any item display record. Helmet and shoulder
creation (`0x004EF0D0`, `0x004EF840`), held-item creation (`0x004EACD0`), and the
component setter (`0x004F2640`) add no animation or reflection behavior for bits
`0x40`, `0x80`, or `0x100`. The character-component function range
`0x004E7300` through `0x004F2900` was also inspected for deferred flag handling against
the fingerprinted executable documented in `tools/ghidra/README.md`.

The previous interpretation of these bits as character sequence inheritance,
opposite-shoulder synchronization, and local-X reflection came from SolCL and
has been removed. It could overwrite an item's animation with the body's
posture and reflect both its geometry and effects. The Vulkan equipment test
uses displays containing all three bits, verifies independent item/effect
animation while a remote character sits, and checks authored orientation.
Generic renderer support for reflected model transforms remains available.

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

## Unit movement requests

The retained unit animation owner now receives local movement notifications
before the final frame snapshot. A jump requests 37; completion selects 38.
`724200` preserves behavior 37--40 or 467 while the nonspline falling gate at
`723350` is active. A zero-launch support acquisition does not enter that gate
until falling-far, so world entry does not manufacture a takeoff animation.

Landing retains the old movement flags and nonzero launch-speed predicate
before the movement owner clears them. The ordinary `73D2B0` rule selects 39
when stationary, or 187 while running forward/sideways above twice walking
speed. Walking/backward/aquatic landings resume the ordinary resolver. The
39 primary blocks idle and turning until completion, but translation can
interrupt it. Ground turn requests use 11/12 with native left precedence and
movement-mode rejection from `71DE90`/`71E180`.

The local event queue is independent of encrypted writer admission. Heartbeat,
facing and pitch notifications do not rerun the primary resolver. Event order,
model/tier fallback, existing sequence blends and the shared CRT variation/cycle
stream remain owned by the CPU unit playback across GPU replacement.

`tools/ghidra/unit_movement_animation_oracle.py` captures 438 original decisions
for ordinary landing, turning, nonspline falling and jump completion. Runtime
tests cover retained takeoff, both landings, landing interruption, short jumps,
multiple notifications in one frame, and random-draw counts. These checks do
not establish incoming remote movement-event handling, directional bone poses,
spline/vehicle controllers, or combat layers.

The opt-in `stock_character_movement_sequences_complete` runtime test exercises
the real installed archives for both genders of all ten playable races. All 20
models complete takeoff, airborne loop, stationary landing, both turns, running
landing and the return to ordinary locomotion using their authored timers. It
also samples finite blended bone palettes through walking, slowed running,
ordinary running, accelerated running and sprint fallback on every model.

### Movement speed and stride phase

The nonspline speed resolver executes the policy recovered from `987570`:
flight precedes swimming, backward speeds are capped by their forward speed,
and walking is capped by running. Translation or vertical movement is required;
turning alone has zero movement speed. After backward/aquatic selection,
`717050` selects sprint 143 at **11 yards/second or higher**, run 5 strictly above
twice the walking speed, and walk 4 otherwise. The walking flag alone does not
determine this selection.

`7385C0` scales an admitted resolved animation ID by actual movement speed
divided by the absolute authored speed of variation ordinal zero. Tiered IDs
outside `714E80`'s whitelist retain rate one. When both old and new metadata
have movement speed and nonzero durations, the new offset is
`(old_phase.wrapping_mul(new_duration) / old_duration) % new_duration`.
The old phase is queried at the current scene tick, before pose clamping or
wrapping, and belongs to the actual outgoing variation. Identical body primary
requests update speed through `827000` with the native tolerance; they retain
the variation and consume no random draws.

Mounts query their own model's ordinal-zero stride and outgoing variation,
using the unit's movement flags and speed. The final mount gate at
`739113..73917C` differs from the body commit: an identical ID is retained only
while the absolute rate difference is at most float `0x3C23D70A` (0.01).
A larger difference submits a new `735820` request, including phase offset,
blend, weighted variation and cycle draws. Creation and local-player,
remote-player and creature updates use this policy. The 240-case
`unit_mount_request_oracle.py` capture records the gate and submitted arguments;
it does not execute upstream behavior resolution or vehicle propagation.
Runtime checks cover the admitted native cases, the resulting timers and random
draws, different ordinal-zero/weighted-variation durations, and all three live
renderer update paths with distinct rider/mount stride metadata.

Primary timers retain speed and the native stored reciprocal. Construction,
rate changes, primary/secondary pose sampling, event keys, completion deadlines
and automatic variation restart preserve the original integer/x87 rounding
boundaries. In particular, completion cycle duration uses a float store and
nearest-even rounding, while construction truncates the wider product. Blends
retain the outgoing timer's speed independently of the incoming primary.

`model_sequence_speed_oracle.py` captures 11,760 original timer cases;
`unit_movement_speed_oracle.py` captures 700 speed/selector cases and 756
speed/stride-policy cases. Runtime tests additionally exercise unchanged RNG
on rate updates, ordinal-zero metadata versus the selected variation, fresh
stride carry, scaled event deadlines, automatic variation remainder, and
independently advancing blended poses. These captures supply model-accessor
inputs where documented; they do not claim full native unit-controller replay.
