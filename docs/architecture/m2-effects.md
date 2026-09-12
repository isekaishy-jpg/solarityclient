# Build-12340 M2 effects

Version-264 M2 bodies store ribbons and particles in top-level arrays at header
offsets `0x120` and `0x128`. Their exact build-12340 record sizes are 176 and
476 bytes. They are not interchangeable with later client layouts.

Solarity's owned decoders read the exact records directly from the archive
bytes and allocate only the retained representation. Larger same-path HD
models are not cloned into a second parser representation.

The same isolation covers build-12340 attachments, their lookup, events,
lights, cameras, and the camera lookup. Later record shapes may not be used as
an implicit conversion or fallback.

## Per-model effect time

The model constructor `0x00834810` initializes `CM2Model +0x8C` from its owning
scene's current millisecond tick. `0x00828A00` subtracts that saved tick with
unsigned wrapping arithmetic, converts milliseconds to seconds, stores the new
tick, and sends the delta to both ribbons and particles. A model that has not
run this update retains its previous timestamp. The zero written by the bare
object allocator is overwritten during scene construction; it is not a rule
that every new effect should catch up from scene entry.

Each runtime M2 placement now owns that timestamp alongside its particles and
ribbons. Static placements, game objects and their WMO doodads, creatures,
characters, equipment, enchantments, and Glue children initialize it at their
scene admission. A backdrop uses zero because its widget owner starts a fresh
local clock at model creation. Retained components keep the entire placement;
body material replacement transfers the timestamp together with its effect
histories. Culled placements do not consume the next effect interval. The
emitter's existing lifetime cap still bounds accumulated simulation work.

`tools/ghidra/model_effect_clock_oracle.py` executes the original constructor
and update arithmetic. Its hooks supply scene/resource dependencies and capture
particle dispatch; it does not simulate particles or establish culling policy.
The eight native probes cover a nonzero creation tick, repeated tick, deferred
update, and 32-bit wraparound. The Vulkan equipment test adds an item 20 seconds
into a scene and expects only 100ms of initial emission, then verifies complete
particle and ribbon aging after an offscreen interval. Material residency
snapshots also include the effect timestamp. Full frame-retirement retention
and the wider runtime's floating-point scene-clock precision remain separate
ownership and clock work.

The constructor also stores a distinct global-track origin at `+0x74`.
Shared playback retains this origin through primary sequence changes and
pauses; each authored global duration uses the unsigned elapsed remainder.
This starts a newly admitted effect's burst at its own creation even in an
older scene. The oracle's optional 240 global-clock probes, decoded emitter
track sampling, and retained-unit material replacement checks cover this
boundary; see [M2 animation](m2-animation.md#per-model-global-tracks).

## Default model sequence

Native load completion (`0x00832EA0`) and scene binding (`0x00834540`)
request Stand through `0x00832AB0` with automatic variation selection, zero
offset, speed one, and no blend. This consumes a weighted variation draw and
a cycle-count draw even when only one variation exists. It does not force
the record whose variation metadata is zero. The AnimationData fallback mode
survives the request, including reverse and held endpoints; the emergency
selection uses animation 147 or the first authored record when Stand is absent.
A model without bones does not start a primary timer or consume either draw.

Glue widget load completion, static MDDF/MODD placement admission, replicated
WMO doodad admission, and equipped items and enchantments in Glue and the world
use this shared default initializer. Their integer timer
starts one tick after the current scene time, and retained owners keep that
timer through GPU/material publication. An owner awaiting its first primary
timer does not dispatch sequence-zero events. The remaining unit body, mount,
pet, and gameplay GameObject startup paths still need their default/request ordering
reconciled; this change does not establish parity for those paths.

Equipment creation (`0x004EAA70`) and enchantment creation (`0x004EA8F0`) call
the ordinary factory at `0x0081F8F0`, which reaches load completion through
`0x00834810` and `0x008359C0`. Neither equipment path adds a primary sequence
request. Attachment binding at `0x00831630` retains the child's own timer.
The Vulkan equipment regression uses a zero-weight first Stand variation and
checks selection of the second, two random draws per new model, and a timer
anchored to creation even when an item is equipped 20 seconds after entry.
World material updates preserve the complete timer without consuming startup
draws. Glue equipment retention across character reconstruction and admission
timing before GPU publication remain separate ownership work.

`tools/ghidra/model_default_sequence_oracle.py` executes eight original-code
probes for weighted startup, variation metadata, fallback modes, and the
zero-bone guard. Hooks suppress old-scene removal and supply the CRT generator;
the harness does not exercise asynchronous resource loading or queued gameplay
requests. Archive-decoded runtime tests check the captured sequence choices,
draw counts, scene deadlines, and preservation across WMO parent changes.

## Model attachments

One build-12340 attachment is exactly 40 bytes. It retains the identifier,
`u16` bone index, otherwise-unknown `u16`
word, bone-relative position, and byte-valued animated enable track. Bone and
lookup references are validated during admission, while `0xFFFF` lookup holes
remain absent rather than triggering an attachment-array search.

The Breath attachment's authored position remains directly available for
stock player-camera height behavior. Placement animation can separately sample
the enable track and bone pose; neither operation mutates the shared decoded
M2, including when a larger same-path HD replacement wins MPQ precedence.

## Model lights

One build-12340 model light is 156 bytes. It contains a `u16` directional/point
selector, an optional signed bone index, a bone-relative position, and seven
nested tracks: ambient RGB/intensity, diffuse RGB/intensity, attenuation
start/end, and byte visibility. The owned decoder resolves those tracks through
the same sequence storage rules as model animation and rejects later light type
selectors rather than coercing them to point lights.

Mutable light state belongs to a model placement. The decoded declaration and
animation keys remain shared with the M2 asset, including when a higher-priority
HD patch supplies a larger file at the same virtual path.

## Model cameras

One build-12340 model camera is exactly 100 bytes. It retains the signed
camera-role selector, field of view,
near/far clip planes, animated position and target offsets with their base
vectors, and animated roll. It has no trailing later-version ID or flag word.

The separate signed camera lookup preserves `-1` as an absent semantic slot and
validates every nonnegative index. Missing roles remain missing; camera zero is
not a compatibility substitute. Runtime presentation samples shared camera
tracks into placement-local state. Position and target use complete 36-byte
value/incoming/outgoing keys, and roll uses 12-byte keys, including for step
and linear interpolation. Roll interpolation preserves authored full turns.

## Model events

One build-12340 event is 36 bytes. It stores a four-byte identifier,
family-specific data word, 32-bit bone
reference, bone-relative position, and a 12-byte timestamp-only nested track.
Both `0xFFFF` and `0xFFFF_FFFF` are retained as stock absent-bone sentinels.

Event timelines use the same internal `.m2`, external `.anim`, alias, and global
clock routing as ordinary tracks, but carry no value array. Trigger timestamps
remain ordered in their authored outer-channel slots so audio, spell, footstep,
and presentation systems can interpret the identifier without format-layer
coercion.

### Mounted-camera markers

Mounted-camera markers are queried declarations, not timeline callbacks. After
the controlled mount's current bone pose is composed, `$CMA` is transformed
through its owning bone and placement; its world Z minus the mount origin's
world Z replaces the principal camera-height target. Changes within 0.05 units
retain the current target. Accepted changes use the stock cosine transition,
the default `cameraHeightSmoothSpeed` value 1.2, and the mount duration factor
0.5, which remains active for three seconds after the last `$CMA` sample.

When `$CMA` is absent, `$CFM` contributes its raw authored local Z as a
separately smoothed flying-mount collision height. It is read once per mount
generation and uses the default `cameraFlyingMountHeightSmoothSpeed` value
2.0. This value remains separate from the principal orbit-pivot height because
build 12340 consumes it while constructing camera obstruction geometry. A
missing marker remains missing; `CreatureModelData.mountHeight`, the rider
attachment, and the other marker are not compatibility substitutes.

These lookups run on the archive-selected mount model. A larger same-path HD
replacement can therefore carry different marker geometry without acquiring a
different asset identity or an HD-only camera path.

## Ribbons

The asset boundary now owns every field of the 176-byte ribbon record:

- emitter ID, bone-relative position, and optional bone owner;
- ordered texture and material index arrays;
- color, fixed16 alpha, height-above, height-below, texture-slot, and byte
  visibility tracks;
- edge rate, edge lifetime, gravity, flipbook rows/columns, priority plane,
  particle-color channel, and texture-transform lookup selector.

Nested tracks use the same internal, external `.anim`, alias, and global-clock
resolution as bones and material animation. Texture, material, color, bone, and
texture-transform references are validated during model admission. Missing or
invalid references do not receive compatibility substitutions.

Rendering samples ribbon color, fixed16 alpha, both heights, the held texture
slot, and byte visibility through the same local/global clock routing used by
bones. Each placed emitter owns its edge history; the decoded declaration and
its BLP source remain shared, including when a larger same-path HD replacement
wins archive precedence.

The build-12340 executable rounds edge rate upward, clamps edge lifetime to
0.25 seconds, and allocates `ceil(rate * lifetime) + 2` edge pairs. The retained
trail reproduces that bound, its nominal one-interval first update, lifetime
expiry, quadratic gravity integration, and the two-handle interpolation used
when a frame crosses an edge boundary. Dynamic mesh preparation emits stock's
24-byte position/color/texture-coordinate vertex layout, including packed BGRA
color and age-progressed coordinates within the selected flipbook cell. Vulkan
frame slots grow one host-visible PCT0 stream to their observed high-water
mark, upload visible placement histories, and submit triangle strips in the
same dynamic-rendering scope and depth attachment as terrain, WMO, and M2
bodies. The scene descriptor is shared with M2 presentation; each parallel
material/texture entry becomes one stock-ordered strip pass.

The emitter remains one scene element. `821A20` reads the first material and
the owner/track alpha to choose the opaque or liquid-dependent transparent
queue. `820F40` selects that element's first material for common setup, then
`980B70` draws all parallel material/texture entries consecutively. Runtime
packets now share one scene-order value across the emitter and retain authored
pass order through stable sorting. Later material blends cannot move part of a
ribbon into another queue or across the water boundary.

The registered model/liquid regression covers alpha-then-opaque and
opaque-then-alpha ribbons, both full and faded owner opacity, and movement
above, through and below a registered WMO liquid. It checks shared packet
order, authored pass order, queue placement and retained/new edge opacity.
Its framebuffer assertions remain scoped to mesh liquid clipping; they do not
claim ribbon fog or composite ribbon/water pixel parity.

The 2026-09-12 grouping change passes all 424 runtime tests (19 explicit
environment-dependent tests ignored) and workspace Clippy with warnings denied.

Owner alpha is multiplied into the sampled ribbon alpha before new edges are
created, matching `828A00` and `97FBA0`. `980090` retains the packed color of
older edges, so changing owner opacity does not recolor the trail's history.
The runtime includes both placement opacity and the animated model color alpha.
Ribbon submission at `980B70` restores each retained material's authored blend,
depth, and static alpha-test states after the common scene setup. Alpha-key
uses GX's `224 * (1 / 255)` reference without owner scaling; no-alpha-add uses
zero. Ribbons therefore do not select the mesh/particle fade pipeline.

The executable indexes the material-state and texture-pointer arrays in
lockstep while drawing. Admission therefore requires equal array lengths
instead of dropping extra entries or substituting pass zero.

## Particles

The asset boundary owns the complete 476-byte WotLK particle record:

- identifier, flags, bone-relative position, optional bone, packed texture
  field, geometry-model path, and recursive child-emitter path;
- blend, emitter, particle, head/tail, priority, flipbook, and
  `ParticleColor.dbc` selectors;
- eleven ordinary emitter-time tracks with full internal/external/alias/global
  sequence resolution;
- five header-less lifetime ramps for color, fixed16 alpha, two-axis scale, and
  head/tail flipbook cells;
- variation, tail, twinkle, drag, spin, tumble, wind, follow, spline, and
  animated enable parameters.

The separate ten-word build-12340 `ParticleColor.dbc` catalog owns its row ID
and three packed start, middle, and end color triplets. A nonzero creature or
item display identifier produces three placement-local setter calls for M2
emitter selectors 11, 12, and 13. Identifier zero leaves the shared authored
colors untouched. The executable deliberately substitutes packed green for a
missing nonzero row; Solarity retains that diagnostic behavior without
applying it to static ADT or WMO doodads.

The resident local-player placement carries its display-selected replacement
into the same particle mesh path as world placements. Static placements retain
`None`; the shared decoded emitter and `ParticleColor.dbc` catalog are never
mutated.

Under flag `0x10000000`, the 16-bit texture field contains three five-bit model
texture indices. Otherwise it is one ordinary index. Admission expands and
validates the applicable representation instead of assuming every emitter has
one texture.

Decoded emitters and their arrays are owned independently of the M2 source
buffer. Particle simulation must own mutable instances per model placement;
the decoded declarations, BLP sources, and GPU resources remain shared. HD
patches follow the same MPQ precedence and virtual asset paths, so larger
payloads do not create another record type or resource identity.

Rendering now samples the eleven emitter-time tracks through the selected M2
and global clocks, while each particle's five lifetime ramps use the stock
signed fixed-16 normalized domain `0x0000..=0x7FFF`. Continuous color, alpha,
and scale ramps interpolate; byte-domain particle RGB becomes normalized only
at the renderer boundary, and integer head/tail flipbook cells interpolate
before rounding to the nearest even integer. A
particle-local stream reseeded from its stored 16-bit word applies shared or
independent scale variation and multiply-high random head-cell selection in
the executable's call order. Initial rotation and angular velocity use their
own particle-word reseed and conditionally skip zero-variation draws, matching
the stock render preparation. Admission rejects out-of-domain or unordered
lifetime timestamps instead of making the interval search order-dependent.

Ordinary particle heads and tails now prepare PNC0T0 vertices from the
executable corner and atlas-coordinate tables. Each live particle samples its
color, scale, rounded head/tail cell, and head rotation. Tails extend opposite
velocity for the authored length, optionally clamp that length to particle
age, and reproduce the executable's short-projection fallback billboard.
The process-wide twinkle table consumes one combined two-call CRT seed during
M2 initialization and generates 128 stock random phases. Active emitters use
the current 32-byte particle pool address plus nearest-even age/speed phase to
select visibility and multiply both billboard axes by the authored base plus
random additive range. Geometry particles remain a separate typed path instead
of being flattened into ordinary quads.
Flag `0x10000` retains its pointer-derived alternating spin direction: adjacent
32-byte ordinary pool slots negate the authored head angle independently of
twinkle visibility. Flag `0x200000` instead aligns heads to negative velocity
in camera space, foreshortens the aligned axis by projected speed over full
speed, and returns to the rotating billboard branch below stock's exact
direction threshold. Flag `0x4000` keeps local head offsets in the transformed
emitter X/Y basis and applies authored spin about transformed local Z instead
of substituting the camera billboard basis.

Particle render state is synthesized through the executable's dedicated blend
table rather than treating the authored byte as a root-material blend id.
Selectors `0`, `1`, `2`, `3`, `4`, `5`, and `10` map to opaque, alpha-key,
alpha, no-alpha-add, alpha-add, modulate, and no-alpha-add respectively; the
stock default branch is opaque. Every particle is two-sided and depth-tested.
Low emitter flags independently enable lighting and fog. The synthesized blend
mode enables depth writes only for opaque and alpha-key particles. Their faded
draws use the common `81FE90` source-alpha blend override while retaining that
depth-write bit. Alpha-key reference scales with the complete owner alpha,
including placement opacity and animated model color. The renderer retains
both pipeline variants and pushes the reference separately for each draw.

`tools/ghidra/effect_material_oracle.py` runs unmodified common and ribbon
submission blocks and their GX helpers for 315 material/pass/alpha inputs.
Decoded-material tests compare the resulting blend, depth, cull and alpha-test
states, including the ribbon shader specialization. The capture executes the
original `833934..8339C1` root-material conversion into a preallocated ribbon
pass record. Allocation, the rest of construction, texture binding and GPU
submission are outside that capture. A separate Vulkan test checks faded opaque
and alpha-key particle blending, alpha discard and retained depth writes using
overlapping cards. The runtime liquid fixture checks old and newly created
ribbon edge alpha across owner-opacity changes.

The 2026-09-11 opacity change passes workspace Clippy with warnings denied and
all 1,313 workspace tests (23 explicit environment-dependent tests ignored).
These controlled checks do not establish complete populated-world effect parity.

### Ribbon material color and culling

`820F40` binds the scene's `Particle_Unlit` effect, initialized by `81F330`.
`980B70` publishes each ribbon pass's lighting bit through `8731C0`, then
`873160` selects the `Color_T1` vertex program. The even variant forwards PCT
color and alpha for an unlit material. The odd variant outputs white, including
alpha, for a material with lighting enabled. Neither variant computes diffuse
lighting. Multiplicative root materials retain the shared model setup's effective
unlit flag. Vulkan now specializes this color selection and applies the authored
two-sided flag to back-face culling.

`tools/ghidra/ribbon_shader_oracle.py` executes the original material conversion,
lighting publication and shader selector for four effective flag combinations,
then renders the selected original BLS programs through Direct3D9. The capture
pins the executable and shader fingerprints; the fixture records RGBA with
nonwhite tint and fractional texture/vertex alpha. Fog and shadows are neutral,
the blend is opaque, and D3D culling is disabled to isolate color selection.
The Vulkan framebuffer regression compares those pixels with scene lights both
zero and overbright. It also reverses strip winding: a culled material must draw
one side and a two-sided material must draw both. This checks cull enablement;
it does not independently establish the stock front-face convention or shadow
receiving.

### Ribbon fog and retained submission state

Ribbon fog now preserves the shared submission state.
`81FB10` publishes fog color/range/exponent through `873210` using the ribbon's
first material. `980B70` toggles fog through `873390` for each pass without
reselecting its color. Disabling fog uploads neutral vertex coefficients but
retains the previous color and active coefficients; a later enabled pass can
restore them even if the first material was unfogged. `7A8440` also publishes
these constants during WMO rendering, selecting the local or outdoor fog bank
and an optional black color through its cached mode. Consequently, an emitter
alone is insufficient to reproduce inherited fog: the replay must follow the
actual WMO/M2/effect submission order. The renderer retains this bank across
frame slots, publishes each source in command order, and gives each ribbon pass
a 32-byte push block. The vertex shader evaluates the original eye-depth fog
equation and exponent before interpolation; the fragment shader blends RGB
toward the retained color while preserving alpha. Scene colors cross the native
packed-byte boundary, and doubled modulation selects `128/255` half-white.
The native zero-initialized bank produces visibility one; this is explicit in
GLSL because `pow(0, 0)` is otherwise undefined.

Publication retains value snapshots and defers coefficient calculation and
packed-color conversion until a ribbon consumes the bank. Mesh, particle and
WMO draws therefore avoid converting an intermediate bank that a later draw
may replace. Startup state, disabled-pass restoration and cross-frame retention
still follow the same native register captures.

WMO publication follows the final physical surface pass. Ordinary missing-MOCV
groups select outdoor fog; ordinary MOCV groups select their group bank.
Unified nontransition callbacks force fog even for an unfogged MOMT material
and select outdoor color for group flags `0x48`. Transition callbacks retain
the material fog enable and selected group bank. This metadata describes the
common registers inherited by later M2 effects; it does not establish complete
WMO surface-shader parity.

`tools/ghidra/ribbon_fog_oracle.py` executes native `81FB10`, `873210` and
`873390`, then renders the original fingerprinted `Color_T1`/`Combiners_Mod`
BLS programs through Direct3D9. Its 72 serial cases cover all seven first-pass
blend modes, near/mid/end/beyond fog depth, two exponents, startup state,
unfogged passes, scene-query disable and restoration across submissions.
The Vulkan regression checks 143 RGBA captures, including a translated and
rotated orthographic camera. A separate framebuffer regression resets the bank
before each of nine model/particle/WMO handoffs and checks the inherited native
pixels; model and particle sources use an instance scene distinct from the
default frame scene. Runtime liquid-order coverage also verifies that exactly
the first authored ribbon pass performs common setup.

The 2026-09-12 color/culling and retained-fog changes pass workspace Clippy with
warnings denied and all 1,319 workspace tests (23 explicit environment-dependent
tests ignored). Combined populated-world comparison remains open.

### Ordinary ribbon shadow exclusion

The shared `Color_T1`/`Combiners_Mod` BLS families contain shadow variants, but
ordinary ribbons do not select them. `821A20` constructs a type-3 element at
`8226EE..822730`, setting its shadow selector at `+0x3C` to zero. Mesh elements
instead run `81F1D0` to compute receiver eligibility. The ribbon's `81FB10`
common setup copies its zero selector into `D43010`, replacing any preceding
mesh shadow state, and `980B70` selects programs through `873160(0)`.

`tools/ghidra/ribbon_shadow_oracle.py` executes that constructor, common
publication, lighting-bit publication and shader selection with poisoned
previous shadow selectors 0/1/2, both filtering modes, zero/four local lights,
and lit/unlit two-sided materials. All 24 cases publish zero shadows, select
unshadowed vertex aliases 0/1/8/9 and pixel aliases 0/4, and render the original
BLS programs. The expanded Vulkan color/culling test checks their RGBA outputs.
Allocation, complete model admission and texture binding remain outside this
instruction capture; the full caller establishes the ordinary type-3 path.
Keeping this ribbon shader independent of world shadow-map descriptors matches
that path. Specialized effect callbacks require their own selection evidence.

### Attached particle card size

Build 12340 `0x0097A390` composes the particle-center transform retained at
`0x00B2D550`. The ordinary head path at `0x0097BE80` transforms each center
through that matrix, then adds the lifetime-sized X/Y offsets directly in
view space. Camera-facing card axes consequently remain independent of the
attachment's scale and rotation. Raw flag `0x20` separately multiplies card
size by the emitter X-axis length retained by `0x0097AC20` at runtime offset
`+0x1EC`; raw flag `0x10` only selects emitter-local particle storage.

Previously, resolving the camera axes through a normalized inverse emitter
matrix and then the forward matrix introduced attachment scale into every
model-space card. This shrank Bloodmage shoulder particles by approximately
0.557 in the observed Blood Elf female attachment pose, although their
size-inheritance flag is unset. Emitters with size inheritance applied that scale twice.
The renderer now uses the camera's unit axes. A decoded-model regression covers
shrinking, growing, rotated, translated, and nonuniform attachment transforms,
both with and without size inheritance.

Local-orientation heads use a different basis. `0x0097A390` extracts the
center-transform 3x3 matrix with `0x004C51B0`, then calls `0x004C5230` to divide
all three axes by the retained emitter X-axis length for model-space particles.
This preserves nonuniform axis ratios while removing shared attachment scale;
raw flag `0x20` still controls the separate size multiplication. The renderer
now follows that division instead of keeping the complete scaled basis.

Every ordinary head and tail branch in `0x0097BE80` writes the same lighting
normal from `0x00B2D540..548`. `0x0097E730` reads the current graphics view
matrix, and `0x0097A390` copies its elements 8, 9, and 10 into that normal.
Those values represent world +Z transformed into view space. The native
submission at `0x0097A580` uses identity view for the completed vertices.
Solarity retains world-space vertices and lights, so its equivalent normal is
world +Z for all ordinary cards, independent of their visible plane. Computing
camera-facing or geometric cross-product normals changed stock lighting.
Regression coverage checks the native view-space normal and nonuniform local
card geometry through rotated attachments and multiple camera directions.

Each placed simulation owns the exact table-driven `CParticleEmitter` random
stream seeded from the composition root's two Visual C++ `rand()` results. Its
pool grows, but never shrinks, to the executable's truncated estimate of
`(rate + rate variation) * (lifetime + lifetime variation) * 1.15`. The planar
and spherical-shell paths retain stock's fractional emission remainder,
randomized in-frame age, signed lifetime word, swap-removal, half-step gravity,
clamped linear drag, and executable epsilon snap. Sphere radius, elevation,
azimuth, z-source aiming, and forced vertical launch keep their distinct random
call order. Model-space particles retain local coordinates; ordinary particles
receive their emitter matrix. Recovered but not-yet-implemented spline,
collision, inherited-velocity, and follow paths return typed errors rather than
falling through to another generator.

The shared planar/sphere emission-rate setter at `0x0097BD80` clamps the
sampled base rate to zero. Signed authored keys remain intact so interpolation
across zero is preserved; clamping keys during decoding changes activation
timing. `GoldPileLarge01.M2` has fifty negative-rate emitters that exposed the
missing setter as a fatal capacity error. Pool sizing at `0x0097EDF0` sets the
x87 rounding-control bits to truncate before `FISTP`; the decompiler's `ROUND`
pseudocode does not mean nearest-even here. Regression coverage checks both
emitter types, negative-to-positive interpolation, fractional pool estimates,
and continued aging of existing particles when the rate falls to zero.

`CM2Model::SetEmission` at `0x008279F0` changes runtime emitter bit 2,
independently of the authored enabled track's bit 1. The placement-local
simulation exposes that switch for CEffect retirement. Clearing it prevents
births and capacity growth without killing existing particles or changing the
fractional emission remainder. Positive-time updates still consume the rate
variation draw and advance live particles. Resetting discontinuous particle
history preserves this model-owned switch.

`CParticleEmitter::Update` at `0x0097DD20` returns before capacity, randomness,
or live-pool work when its elapsed slice is zero. Repeated presentations within
one scene millisecond and zero remainders after exact 100 ms subdivisions now
preserve the random stream. Both emitter shapes have regression coverage for
repeated ticks, disabling births, continued motion, full lifetime expiry, and
resuming emission with the retained random and fractional-count history.

`validate_world_particles` replays sequence zero for sixty seconds at 60 Hz
across the selected ADT's 3-by-3 neighborhood, including active WMO doodad sets.
It uses fixed seeds, identity emitter matrices, and full density to exercise
installed emission data without a window or server. It does not measure live
world performance or reproduce placement visibility and process random history.

The spherical spawn path at `0x00981950` retains its sampled angular direction
before multiplying it by the shell radius. Zero-radius emitters therefore
launch moving particles from a common center; a negative radius changes the
birth position without reversing that direction. Deriving direction from the
scaled birth position previously stopped Bloodmage hood and shoulder particles,
whose radius is zero despite an authored speed of `1/36` units per second.
The z-source branch separately normalizes its aim only when squared length
exceeds the exact `0x009EA27C` constant (`0x34800000`); smaller vectors remain
unchanged. Regression tests cover both branches through decoded emitters and
live simulation.

Resident M2 generations share one pipeline and one sampled-image descriptor per
ordinary emitter, while every MDDF or MODD placement owns its simulation and
random stream. Visible planar and spherical emitters sample the placement's
current animation clock and bone matrix, append PNC0T0 vertices and `u32`
indices to the grow-only world-frame ring, and issue indexed draws beside M2
bodies and ribbons. Flag `0x200` keeps live state in emitter space and applies
the current animated transform during mesh preparation; all other ordinary
particles retain the world-space position chosen when they were emitted.
Multi-texture, geometry, child-emitter, and other specialized paths remain
typed boundaries and never substitute the one-texture ordinary shader.

### Particle generator basis

The model owner at build-12340 `0x008309C0` copies the animated bone matrix,
appends the authored emitter position with `0x004C1B30`, and applies the model
placement with `0x004C2370`. It then appends the fixed matrix initialized at
`0x00D411E0` before calling the particle driver at `0x0097EB10`. Recovered
`0x004C1F00` multiplication order and the `-1.0` constant at `0x009E2EF4`
establish the generator remap: local +X becomes bone +Y, +Y becomes -X, and
+Z stays +Z. The translation is unchanged by this final basis rotation.

`M2BonePose::particle_emitter_transform` owns that composition for both the
live scene and installed-data diagnostics. World-space particles use it at
birth; model-space particles also use it when preparing their current cards.
Unbound emitters omit the bone transform and keep the same generator remap.
An external regression exercises decoded parent-bone translation, a rotated
and nonuniformly scaled placement, and directed spherical births in both
storage spaces.

The previously omitted rotation directed both foreground Night Elf dust
emitters sideways out of the native camera. With deterministic seeds, 60 Hz
updates, and the authored 16:9 camera, neither emitter produced an in-frustum
vertex at 5, 10, 15, or 20 seconds, and every triangle lay entirely outside
at least one clip plane. The corrected frame produces in-frustum vertices at
all four samples while preserving their live-particle counts.
`validate_m2_particles <Data directory> <locale> <M2 path>` reports those CPU
geometry bounds, alpha coverage, vertex counts, and triangle clip-plane
rejection over 20 seconds. Surviving triangles are only candidates: this does
not measure GPU occlusion, blended pixels, or equivalence to a stock capture.

## September 12: street-brazier smoke flipbook

The reported Orgrimmar gate brazier is `OrcBrazierStreetLamp.m2`. Its smoke
emitter uses `Spells/ToonSmoke16.blp`, an 8-by-8 atlas. The head-frame ramp has
keys `[0, 16384, 16384, 32767]` and cells `[0, 16, 17, 33]`. Holding the lower
key kept the early atlas frame for almost half of each particle's 3.725-second
life instead of advancing through the smoke animation.

Native `979560` interpolates unsigned frame values through `9793B0` and
`979330`, stores the result to float32, and uses nearest-even `FISTP` rounding.
Two-key ramps use normalized age directly; three-key ramps split at the middle
normalized timestamp. General ramps retain duplicate-key transitions. Both
head and tail selection now follow that path. Random-cell selection for an
absent head ramp retains its previous PRNG ordering.

`particle_flipbook_oracle.py` executes those original functions without hooks.
Its 792 cases cover single, two-key, three-key, descending, uneven, and actual
brazier-style duplicate-midpoint ramps, including rounding ties and endpoint
samples. Tests decode matching M2 ramps and compare both head and tail cells.
This checks lifetime frame selection; a static screenshot cannot establish
matching process-wide random histories or complete smoke motion.
