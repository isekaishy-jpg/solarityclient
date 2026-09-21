//! Renderer-validated commands own only numeric Vulkan state; the scoped batch pins resources.

#![allow(unsafe_code)]

use ash::{Device, vk};

/// A complete draw is independent of state left by any preceding command buffer.
#[derive(Clone, Copy)]
pub(in crate::device::vulkan_world_frame) struct SceneCommand {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub vertex: (vk::Buffer, vk::DeviceSize),
    pub index: Option<(vk::Buffer, vk::DeviceSize, vk::IndexType)>,
    pub sets: [vk::DescriptorSet; 5],
    pub set_count: usize,
    pub offsets: [u32; 2],
    pub offset_count: usize,
    pub pushes: [u8; 128],
    pub push_count: usize,
    pub stages: vk::ShaderStageFlags,
    pub count: u32,
    pub first: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
    pub instances: u32,
    pub timestamp: Option<(vk::QueryPool, u32)>,
}

impl Default for SceneCommand {
    fn default() -> Self {
        Self {
            pipeline: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            vertex: (vk::Buffer::null(), 0),
            index: None,
            sets: [vk::DescriptorSet::null(); 5],
            set_count: 0,
            offsets: [0; 2],
            offset_count: 0,
            pushes: [0; 128],
            push_count: 0,
            stages: vk::ShaderStageFlags::empty(),
            count: 0,
            first: 0,
            vertex_offset: 0,
            first_instance: 0,
            instances: 1,
            timestamp: None,
        }
    }
}

impl SceneCommand {
    /// Copies a validated layout's bounded descriptors, offsets and push range.
    pub(in crate::device::vulkan_world_frame) fn parameters(
        mut self,
        sets: &[vk::DescriptorSet],
        offsets: &[u32],
        pushes: &[u8],
        stages: vk::ShaderStageFlags,
    ) -> Self {
        self.sets[..sets.len()].copy_from_slice(sets);
        self.set_count = sets.len();
        self.offsets[..offsets.len()].copy_from_slice(offsets);
        self.offset_count = offsets.len();
        self.pushes[..pushes.len()].copy_from_slice(pushes);
        self.push_count = pushes.len();
        self.stages = stages;
        self
    }
}

/// Binding reuse belongs to one command buffer, never a worker or a global registry.
#[derive(Default)]
pub(in crate::device::vulkan_world_frame) struct SceneBindings {
    pipeline: vk::Pipeline,
    vertex: Option<(vk::Buffer, vk::DeviceSize)>,
    index: Option<(vk::Buffer, vk::DeviceSize, vk::IndexType)>,
}

impl SceneBindings {
    /// Records only previously validated numeric state into the exclusively owned buffer.
    pub(in crate::device::vulkan_world_frame) fn record(
        &mut self,
        device: &Device,
        buffer: vk::CommandBuffer,
        draw: &SceneCommand,
    ) {
        // SAFETY: Admission validates every resource and range. The pending owner
        // pins their registries/slot through CPU completion and the GPU slot fence.
        unsafe {
            if let Some((pool, index)) = draw.timestamp {
                device.cmd_write_timestamp(
                    buffer,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    pool,
                    index,
                );
                return;
            }
            if self.pipeline != draw.pipeline {
                device.cmd_bind_pipeline(buffer, vk::PipelineBindPoint::GRAPHICS, draw.pipeline);
                self.pipeline = draw.pipeline;
            }
            if self.vertex != Some(draw.vertex) {
                device.cmd_bind_vertex_buffers(buffer, 0, &[draw.vertex.0], &[draw.vertex.1]);
                self.vertex = Some(draw.vertex);
            }
            if let Some(index) = draw.index
                && self.index != Some(index)
            {
                device.cmd_bind_index_buffer(buffer, index.0, index.1, index.2);
                self.index = Some(index);
            }
            device.cmd_bind_descriptor_sets(
                buffer,
                vk::PipelineBindPoint::GRAPHICS,
                draw.layout,
                0,
                &draw.sets[..draw.set_count],
                &draw.offsets[..draw.offset_count],
            );
            if draw.push_count != 0 {
                device.cmd_push_constants(
                    buffer,
                    draw.layout,
                    draw.stages,
                    0,
                    &draw.pushes[..draw.push_count],
                );
            }
            if draw.index.is_some() {
                device.cmd_draw_indexed(
                    buffer,
                    draw.count,
                    draw.instances,
                    draw.first,
                    draw.vertex_offset,
                    draw.first_instance,
                );
            } else {
                device.cmd_draw(
                    buffer,
                    draw.count,
                    draw.instances,
                    draw.first,
                    draw.first_instance,
                );
            }
        }
    }
}
