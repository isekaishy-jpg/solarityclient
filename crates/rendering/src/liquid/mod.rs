//! Liquid chunk geometry, water and magma materials, and wave presentation boundaries.

mod depth;
mod depth_texture;
mod shader_uniform;
mod terrain_mesh;
mod texture_animation;
mod texture_transform;

pub use depth::LiquidDepthCoordinates;
pub use depth_texture::{LiquidDepthTexture, LiquidDepthTextureKind};
pub use shader_uniform::{LiquidFog, LiquidLighting, LiquidPointLight, LiquidShaderUniform};
pub use terrain_mesh::{LiquidRenderVertex, TerrainLiquidMeshPlan};
pub use texture_animation::LiquidTextureTimeline;
pub use texture_transform::{
    LiquidScrollError, liquid_magma_surface_transform, liquid_water_surface_transform,
};
