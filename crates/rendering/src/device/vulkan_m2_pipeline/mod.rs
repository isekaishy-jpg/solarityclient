//! Renderer-local Vulkan pipelines for stock M2 effect permutations.

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::M2PipelineRegistry;
pub use types::{M2PipelineHandle, M2PipelineInfo};
