//! Embedded GLSL translation of the stock ordinary-particle PNC0T0 path.

pub(super) const PARTICLE_VERTEX_SOURCE: &str = include_str!("source/m2_particle.vert.glsl");
pub(super) const PARTICLE_FRAGMENT_SOURCE: &str = include_str!("source/m2_particle.frag.glsl");
