//! Typed liquid geometry, material pipelines, and frame-slot draw resources.

mod draw;
mod frame;
mod frame_image;
mod mesh;
mod pipeline;
mod pipelines;

pub use draw::{LiquidDrawMaterial, LiquidFrame, LiquidPreparedDraw};
pub(in crate::device) use frame::LiquidFrameResources;
pub use mesh::LiquidMeshHandle;
pub(in crate::device) use mesh::LiquidMeshRegistry;
pub(in crate::device) use pipelines::LiquidPipelines;
