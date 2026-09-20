//! Compact immutable command inputs contain no borrowed registry or mutable renderer.

use crate::VulkanError;
use ash::{Device, vk};
use solarity_cpu::CpuBuffer;

/// A complete indexed caster command after all resource joins have been validated.
#[derive(Clone, Copy)]
pub(super) struct CasterCommand {
    pub(super) pipeline: vk::Pipeline,
    pub(super) layout: vk::PipelineLayout,
    pub(super) vertex: vk::Buffer,
    pub(super) indices: vk::Buffer,
    pub(super) sets: [vk::DescriptorSet; 4],
    pub(super) set_count: usize,
    pub(super) dynamic_offset: u32,
    pub(super) index_type: vk::IndexType,
    pub(super) first_index: u32,
    pub(super) index_count: u32,
    pub(super) first_instance: u32,
    pub(super) instance_count: u32,
    pub(super) push_constants: Option<[u8; 16]>,
}

/// All attachment state needed to reproduce one original shadow rendering scope.
#[derive(Clone, Copy)]
pub(super) struct ShadowPass {
    pub(super) color_image: vk::Image,
    pub(super) color_view: vk::ImageView,
    pub(super) depth_image: vk::Image,
    pub(super) depth_view: vk::ImageView,
    pub(super) rect: vk::Rect2D,
    pub(super) primary: bool,
}

/// A pass owns its command buffer exclusively until the CPU phase returns it.
#[derive(Default)]
pub(super) struct ShadowJob {
    pub(super) pass_index: usize,
    pub(super) device: Option<Device>,
    pub(super) command: vk::CommandBuffer,
    pub(super) pass: Option<ShadowPass>,
    pub(super) draws: CpuBuffer<CasterCommand>,
    pub(super) query_pool: Option<vk::QueryPool>,
    pub(super) initialize: [vk::Image; 6],
    pub(super) result: Option<Result<(), VulkanError>>,
}

/// Submission retains the original primary/environment order regardless of CPU finish order.
#[derive(Default)]
pub(in super::super) struct ShadowSubmission {
    pub(in super::super) commands: [vk::CommandBuffer; 4],
    pub(in super::super) count: usize,
}
