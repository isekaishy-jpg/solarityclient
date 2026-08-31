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
not a compatibility substitute. Runtime presentation will sample shared camera
tracks into placement-local state rather than mutate the decoded M2.

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
and scale ramps interpolate; integer head/tail flipbook cells remain held. A
particle-local stream reseeded from its stored 16-bit word applies shared or
independent scale variation and multiply-high random head-cell selection in
the executable's call order. Admission rejects out-of-domain or unordered
lifetime timestamps instead of making the interval search order-dependent.

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
