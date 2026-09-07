//! Streamed liquid assets and their world-frame presentation.

mod assets;
mod environment;
mod gpu;
mod terrain;

#[cfg(test)]
#[path = "../../../tests/application/liquid.rs"]
mod tests;

pub use assets::RuntimeLiquidAssetError;
pub(in crate::application) use assets::{
    LiquidAssetCache, ResidentLiquidMaterial, ResidentLiquidShader, ResidentLiquidSurface,
};
pub(in crate::application) use environment::{liquid_depth_images, liquid_environment};
pub(in crate::application) use gpu::{LiquidGpuMaterialCache, TerrainLiquidGpuBatch};
pub(in crate::application) use terrain::{ResidentTerrainLiquidBatch, prepare_terrain_liquids};
