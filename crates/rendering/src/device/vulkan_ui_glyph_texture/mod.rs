//! Renderer-local coverage atlases generated from archive-backed UI fonts.

mod registry;
mod types;

pub(in crate::device) use registry::UiGlyphTextureRegistry;
pub use types::{UiGlyphTextureHandle, UiGlyphTextureResourceInfo};
