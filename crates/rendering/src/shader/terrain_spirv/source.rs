//! Embedded GLSL translation of the stock build-12340 terrain path.

/// World-space vertex transform, lighting inputs, and atlas addressing.
pub(super) const TERRAIN_VERTEX_SOURCE: &str = include_str!("source/terrain.vert.glsl");

/// Ordered four-layer splat composition with baked MCSH attenuation.
pub(super) const TERRAIN_FRAGMENT_SOURCE: &str = include_str!("source/terrain.frag.glsl");
