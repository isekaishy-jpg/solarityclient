//! Build inputs and embedded SPIR-V for the stock ordinary-particle PNC0T0 path.

pub(super) const PARTICLE_VERTEX_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-particle.vert.spv"));
pub(super) const PARTICLE_FRAGMENT_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-particle.frag.spv"));
