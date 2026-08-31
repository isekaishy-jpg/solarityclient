# Build-12340 M2 effects

Version-264 M2 bodies store ribbons and particles in top-level arrays at header
offsets `0x120` and `0x128`. Their exact build-12340 record sizes are 176 and
476 bytes. They are not interchangeable with the later layouts exposed by the
current `wow-m2` dependency.

The dependency parser therefore receives a temporary model-header view with
texture-animation, ribbon, and particle arrays hidden. The original header is
restored immediately afterward. Solarity's owned decoders then read the exact
records directly from the original bytes. This in-place parser view avoids
cloning an HD-sized M2 solely to bypass incompatible dependency structures.

The same isolation covers build-12340 events, lights, cameras, and the camera
lookup. `wow-m2 0.7` exposes later record shapes for those arrays; none may be
used as an implicit conversion or fallback.

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

One build-12340 model camera is exactly 100 bytes, not the dependency's later
108-byte structure. It retains the signed camera-role selector, field of view,
near/far clip planes, animated position and target offsets with their base
vectors, and animated roll. It has no trailing later-version ID or flag word.

The separate signed camera lookup preserves `-1` as an absent semantic slot and
validates every nonnegative index. Missing roles remain missing; camera zero is
not a compatibility substitute. Runtime presentation will sample shared camera
tracks into placement-local state rather than mutate the decoded M2.

## Model events

One build-12340 event is 36 bytes, not the dependency's 44-byte range-based
record. It stores a four-byte identifier, family-specific data word, 32-bit bone
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

The decoder does not yet simulate edge history or submit ribbon geometry. That
state must be owned per placement, while decoded tracks, BLP sources, and GPU
resources remain shared—including larger same-path HD replacements.

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
