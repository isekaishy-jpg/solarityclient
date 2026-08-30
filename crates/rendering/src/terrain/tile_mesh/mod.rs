//! ADT-wide geometry and material-atlas preparation for one transfer.

mod prepare;
mod types;

pub use types::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainTileMeshPlan, TerrainTileMeshPlanError,
};
