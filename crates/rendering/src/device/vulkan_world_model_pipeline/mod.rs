//! Cacheable Vulkan graphics pipelines for stock MapObj effects.

mod pipeline;
mod registry;
mod types;

pub use types::{WorldModelPipelineHandle, WorldModelPipelineInfo};

pub(in crate::device) use registry::WorldModelPipelineRegistry;
