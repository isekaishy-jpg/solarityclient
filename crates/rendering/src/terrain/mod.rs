//! GPU terrain, liquid, shadow, doodad, and low-detail map rendering.
//!
//! The boundary follows `MapChunk.cpp`, `MapChunkLiquid.cpp`, `MapShadow.cpp`,
//! `MapLowDetail.cpp`, and `DetailDoodad.cpp`. Decoded map data is borrowed from
//! the asset facade.

mod detail_doodad;
mod gpu_state;
mod low_detail;
mod map_weather;
mod mesh;
mod tile_mesh;

pub use gpu_state::TerrainSceneUniform;
pub use low_detail::{TerrainLowDetailMap, TerrainLowDetailMesh, WorldLowDetailFrame};
pub use mesh::{TerrainChunkMeshPlan, TerrainRenderVertex};
pub use tile_mesh::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainTileMeshPlan, TerrainTileMeshPlanError,
};
