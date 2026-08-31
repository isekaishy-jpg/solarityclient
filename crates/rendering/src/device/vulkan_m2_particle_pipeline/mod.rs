//! Renderer-local Vulkan pipelines for stock M2 particle materials.

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::M2ParticlePipelineRegistry;
pub use types::{M2ParticlePipelineHandle, M2ParticlePipelineInfo};
