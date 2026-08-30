//! Terrain graphics pipelines keyed by authored MCNK layer count.

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::TerrainPipelineRegistry;
pub use types::{TerrainPipelineHandle, TerrainPipelineInfo};
