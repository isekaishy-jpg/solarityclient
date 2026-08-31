//! Renderer-local Vulkan pipelines for stock M2 ribbon materials.

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::M2RibbonPipelineRegistry;
pub use types::{M2RibbonPipelineHandle, M2RibbonPipelineInfo};
