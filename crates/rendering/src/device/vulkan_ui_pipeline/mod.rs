//! Renderer-local Vulkan pipelines for stock simple-render material variants.

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::UiPipelineRegistry;
pub use types::{UiPipelineHandle, UiPipelineInfo};
