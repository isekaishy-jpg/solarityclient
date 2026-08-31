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
