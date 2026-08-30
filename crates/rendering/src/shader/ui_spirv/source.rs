//! Embedded GLSL translations of the stock simple-render texture path.

/// Logical-canvas transform and fixed 32-byte UI vertex ABI.
pub(super) const UI_VERTEX_SOURCE: &str = include_str!("source/ui.vert.glsl");

/// Texture-modulated and vertex-color-only fragment variants.
pub(super) const UI_FRAGMENT_SOURCE: &str = include_str!("source/ui.frag.glsl");
