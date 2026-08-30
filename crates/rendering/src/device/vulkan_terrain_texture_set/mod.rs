//! Persistent descriptors joining one atlas with one through four diffuse BLPs.

mod registry;
mod types;

pub(in crate::device) use registry::TerrainTextureSetRegistry;
pub use types::{TerrainTextureSet, TerrainTextureSetHandle, TerrainTextureSetInfo};
