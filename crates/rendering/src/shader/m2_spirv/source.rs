//! Build inputs and embedded SPIR-V for the stock BLS variant space.

pub(super) const M2_VERTEX_SPIRV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/m2.vert.spv"));
pub(super) const M2_DIRECT_FRAGMENT_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-direct.frag.spv"));
pub(super) const M2_PCF_FRAGMENT_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-pcf.frag.spv"));
