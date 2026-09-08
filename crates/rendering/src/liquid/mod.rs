//! Liquid chunk geometry, water and magma materials, and wave presentation boundaries.

mod depth;
mod depth_texture;
mod ripple_projection;
mod ripple_vertex;
mod shader_uniform;
mod terrain_mesh;
mod texture_animation;
mod texture_transform;
mod underwater_vertex;
mod world_model_mesh;

pub use depth::LiquidDepthCoordinates;
pub use depth_texture::{LiquidDepthTexture, LiquidDepthTextureKind};
pub use ripple_projection::{WaterRippleProjectionError, water_ripple_surface_transform};
pub use ripple_vertex::{WaterRippleRenderVertex, WaterRippleVertexError};
pub use shader_uniform::{LiquidFog, LiquidLighting, LiquidPointLight, LiquidShaderUniform};
pub use terrain_mesh::{LiquidRenderVertex, TerrainLiquidMeshPlan};
pub use texture_animation::LiquidTextureTimeline;
pub use texture_transform::{
    LiquidScrollError, liquid_magma_surface_transform, liquid_water_surface_transform,
};
pub use underwater_vertex::{UnderwaterParticleVertex, UnderwaterParticleVertexError};
pub use world_model_mesh::{
    WorldModelLiquidDepthColumn, WorldModelLiquidMeshError, WorldModelLiquidMeshPlan,
    WorldModelLiquidSurface,
};
