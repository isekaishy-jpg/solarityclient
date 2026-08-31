//! Placement-owned Vulkan images for composed character body textures.

mod registry;
mod types;

pub(in crate::device) use registry::CharacterAtlasTextureRegistry;
pub use types::{CharacterAtlasTextureHandle, CharacterAtlasTextureResourceInfo};
