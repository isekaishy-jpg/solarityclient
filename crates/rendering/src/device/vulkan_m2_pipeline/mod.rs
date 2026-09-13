//! Renderer-local Vulkan pipelines for stock M2 effect permutations.

use ash::vk;

mod pipeline;
mod registry;
mod types;

pub(in crate::device) use registry::M2PipelineRegistry;
pub use types::{M2PipelineHandle, M2PipelineInfo};

/// Shared set-two ABI. Packed instances are selected by gl_InstanceIndex.
pub(in crate::device) const M2_MATERIAL_DESCRIPTOR_TYPE: vk::DescriptorType =
    vk::DescriptorType::STORAGE_BUFFER_DYNAMIC;
