//! Liquid chunk geometry, water and magma materials, and wave presentation boundaries.

mod depth;
mod depth_texture;
mod texture_animation;

pub use depth::LiquidDepthCoordinates;
pub use depth_texture::{LiquidDepthTexture, LiquidDepthTextureKind};
pub use texture_animation::LiquidTextureTimeline;
