//! Deterministic terrain-detail placement and dedicated stock mesh preparation.

mod mesh;
mod placement;
mod scatter;

pub use mesh::{GroundDetailBatch, GroundDetailMeshPlan, GroundDetailModel, GroundDetailVertex};
pub use placement::{
    GroundDetailDensity, GroundDetailError, GroundDetailPlacement, TerrainDetailChunk,
};
