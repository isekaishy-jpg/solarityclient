//! Renderer-owned device-local ADT geometry and typed resource handles.

mod registry;
mod types;

pub(in crate::device) use registry::TerrainMeshRegistry;
pub use types::{TerrainMeshHandle, TerrainMeshResourceInfo};
