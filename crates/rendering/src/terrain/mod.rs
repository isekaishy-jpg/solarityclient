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
mod shadow;
mod texture_animation;
mod tile_mesh;

pub use detail_doodad::{
    GroundDetailBatch, GroundDetailDensity, GroundDetailError, GroundDetailMeshPlan,
    GroundDetailModel, GroundDetailPlacement, GroundDetailVertex, TerrainDetailChunk,
};
pub use gpu_state::TerrainSceneUniform;
pub use low_detail::{
    TerrainLowDetailMap, TerrainLowDetailMesh, WorldHorizonScale, WorldLowDetailFrame,
};
pub use mesh::{TerrainChunkMeshPlan, TerrainRenderVertex};
pub use shadow::{WorldShadowProjection, WorldShadowProjectionError, WorldShadowQuality};
pub use texture_animation::TerrainTextureAnimationState;
pub use tile_mesh::{
    TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TERRAIN_MATERIAL_ATLAS_WIDTH, TerrainChunkDrawPlan,
    TerrainTileMeshPlan, TerrainTileMeshPlanError,
};
