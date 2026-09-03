//! Build inputs and embedded SPIR-V for the stock ribbon PCT0 path.

pub(super) const RIBBON_VERTEX_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-ribbon.vert.spv"));
pub(super) const RIBBON_FRAGMENT_SPIRV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/m2-ribbon.frag.spv"));
