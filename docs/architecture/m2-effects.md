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
at the renderer boundary, and integer head/tail flipbook cells remain held. A
particle-local stream reseeded from its stored 16-bit word applies shared or
independent scale variation and multiply-high random head-cell selection in
the executable's call order. Initial rotation and angular velocity use their
own particle-word reseed and conditionally skip zero-variation draws, matching
the stock render preparation. Admission rejects out-of-domain or unordered
lifetime timestamps instead of making the interval search order-dependent.

Ordinary particle heads and tails now prepare PNC0T0 vertices from the
executable corner and atlas-coordinate tables. Each live particle samples its
color, scale, held head/tail cell, and head rotation. Tails extend opposite
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
Low emitter flags independently enable lighting, fog, and depth writes.

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
pool grows, but never shrinks, to the executable's nearest-even estimate of
`(rate + rate variation) * (lifetime + lifetime variation) * 1.15`. The planar
and spherical-shell paths retain stock's fractional emission remainder,
randomized in-frame age, signed lifetime word, swap-removal, half-step gravity,
clamped linear drag, and executable epsilon snap. Sphere radius, elevation,
azimuth, z-source aiming, and forced vertical launch keep their distinct random
call order. Model-space particles retain local coordinates; ordinary particles
receive their emitter matrix. Recovered but not-yet-implemented spline,
collision, inherited-velocity, and follow paths return typed errors rather than
falling through to another generator.

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
