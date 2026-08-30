//! Embedded GLSL translations specialized into the stock BLS variant space.

/// Vertex translation shared by the seven coordinate effects and 30 unshadowed variants.
pub(super) const M2_VERTEX_SOURCE: &str = include_str!("source/m2.vert.glsl");

/// Fragment translation shared by all 23 model combiners and alpha-test variants.
pub(super) const M2_FRAGMENT_SOURCE: &str = include_str!("source/m2.frag.glsl");
