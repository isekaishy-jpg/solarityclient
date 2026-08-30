//! Renderer-owned terrain blend/shadow atlases and typed resource handles.

mod registry;
mod types;

pub(in crate::device) use registry::TerrainMaterialRegistry;
pub use types::{TerrainMaterialHandle, TerrainMaterialResourceInfo};
