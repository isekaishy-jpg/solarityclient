//! Worker-side Vulkan recording through one exclusively owned command pool.

#![allow(unsafe_code)]

use super::types::{CasterCommand, ShadowJob, ShadowPass};
use crate::VulkanError;
use ash::{Device, vk};

impl ShadowJob {
    /// Required pass work always finishes; cancellation cannot leave a partial GPU submission.
    pub(super) fn execute(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        let _profile = solarity_profiling::profile!("rendering.shadow.record");
        let _cycles = solarity_profiling::profile_cycles!("rendering.shadow.record_cpu");
        context.diagnostic_value("rendering.shadow.commands", self.draws.len() as u64);
        let started = self.measurement.start();
        let result = self.record();
        let outcome = if result.is_ok() {
            self.measurement.finish(started);
            solarity_cpu::JobOutcome::Succeeded
        } else {
            solarity_cpu::JobOutcome::Failed
        };
        self.result = Some(result);
        outcome
    }

    /// CPU completion precedes any submission or reset of the returned command buffer.
    fn record(&self) -> Result<(), VulkanError> {
        let device = self
            .device
            .as_ref()
            .unwrap_or_else(|| unreachable!("admitted shadow job owns device functions"));
        let pass = self
            .pass
            .unwrap_or_else(|| unreachable!("admitted shadow job owns a pass"));
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: This job exclusively owns a reset pool whose prior GPU use retired.
        unsafe { device.begin_command_buffer(self.command, &begin) }
            .map_err(|error| VulkanError::operation("begin shadow recording", error))?;
        if let Some(pool) = self.query_pool {
            // SAFETY: Primary shadow work is first in submission order; its slot fence protects queries.
            unsafe {
                device.cmd_reset_query_pool(
                    self.command,
                    pool,
                    0,
                    super::super::gpu_profile::QUERY_COUNT as u32,
                );
                device.cmd_write_timestamp(
                    self.command,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    pool,
                    0,
                );
            }
        }
        record_pass(device, self.command, pass, &self.draws);
        for image in self.initialize {
            if image != vk::Image::null() {
                initialize(device, self.command, image);
            }
        }
        // SAFETY: The pass and initialization scopes ended and this buffer remains exclusively owned.
        unsafe { device.end_command_buffer(self.command) }
            .map_err(|error| VulkanError::operation("end shadow recording", error))
    }
}

/// Reproduces the original attachment barriers and per-pass viewport without registry access.
fn record_pass(
    device: &Device,
    command: vk::CommandBuffer,
    pass: ShadowPass,
    draws: &[CasterCommand],
) {
    let mut color_barrier = vk::ImageMemoryBarrier2::default()
        .image(pass.color_image)
        .subresource_range(range(vk::ImageAspectFlags::COLOR))
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
    let depth_stages = vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
    let mut depth_barrier = vk::ImageMemoryBarrier2::default()
        .image(pass.depth_image)
        .subresource_range(range(
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        ))
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .dst_stage_mask(depth_stages)
        .dst_access_mask(
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
    if !pass.primary {
        color_barrier = color_barrier
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_stage_mask(
                vk::PipelineStageFlags2::FRAGMENT_SHADER
                    | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            )
            .src_access_mask(
                vk::AccessFlags2::SHADER_SAMPLED_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            )
            .dst_access_mask(
                vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        depth_barrier = depth_barrier
            .src_stage_mask(depth_stages)
            .src_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE);
    }
    let barriers = [color_barrier, depth_barrier];
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(pass.color_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue { float32: [1.; 4] },
        })];
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(pass.depth_view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.,
                stencil: 0,
            },
        });
    let rendering = vk::RenderingInfo::default()
        .render_area(pass.rect)
        .layer_count(1)
        .color_attachments(&color)
        .depth_attachment(&depth)
        .stencil_attachment(&depth);
    let viewport = vk::Viewport {
        x: pass.rect.offset.x as f32,
        y: pass.rect.offset.y as f32 + pass.rect.extent.height as f32,
        width: pass.rect.extent.width as f32,
        height: -(pass.rect.extent.height as f32),
        min_depth: 0.,
        max_depth: 1.,
    };
    // SAFETY: Captured attachments and descriptors are pinned by the renderer borrow;
    // this job alone records its pool. Ordered submission retains inter-pass image barriers.
    unsafe {
        device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&barriers),
        );
        device.cmd_begin_rendering(command, &rendering);
        device.cmd_set_viewport(command, 0, &[viewport]);
        device.cmd_set_scissor(command, 0, &[pass.rect]);
    }
    record_draws(device, command, draws);
    let barrier = [vk::ImageMemoryBarrier2::default()
        .image(pass.color_image)
        .subresource_range(range(vk::ImageAspectFlags::COLOR))
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
    // SAFETY: The attachment scope ends before following scene commands sample the color map.
    unsafe {
        device.cmd_end_rendering(command);
        device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&barrier),
        );
    }
}

/// Retains binding state within one pass; each compact command preserves original instance offsets.
fn record_draws(device: &Device, command: vk::CommandBuffer, draws: &[CasterCommand]) {
    let mut pipeline = vk::Pipeline::null();
    let mut vertex = vk::Buffer::null();
    let mut index = None;
    for draw in draws {
        // SAFETY: Capture validated every handle and range while the renderer pins their owners.
        unsafe {
            if pipeline != draw.pipeline {
                device.cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, draw.pipeline);
                pipeline = draw.pipeline;
            }
            if vertex != draw.vertex {
                device.cmd_bind_vertex_buffers(command, 0, &[draw.vertex], &[0]);
                vertex = draw.vertex;
            }
            if index != Some((draw.indices, draw.index_type)) {
                device.cmd_bind_index_buffer(command, draw.indices, 0, draw.index_type);
                index = Some((draw.indices, draw.index_type));
            }
            device.cmd_bind_descriptor_sets(
                command,
                vk::PipelineBindPoint::GRAPHICS,
                draw.layout,
                0,
                &draw.sets[..draw.set_count],
                &[draw.dynamic_offset],
            );
            if let Some(push) = &draw.push_constants {
                device.cmd_push_constants(
                    command,
                    draw.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    push,
                );
            }
            device.cmd_draw_indexed(
                command,
                draw.index_count,
                draw.instance_count,
                draw.first_index,
                0,
                draw.first_instance,
            );
        }
    }
}

/// New cache images receive visibility one before any environment pass or receiver samples them.
fn initialize(device: &Device, command: vk::CommandBuffer, image: vk::Image) {
    let color = range(vk::ImageAspectFlags::COLOR);
    let before = [vk::ImageMemoryBarrier2::default()
        .image(image)
        .subresource_range(color)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .dst_stage_mask(vk::PipelineStageFlags2::CLEAR)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
    let after = [vk::ImageMemoryBarrier2::default()
        .image(image)
        .subresource_range(color)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_stage_mask(vk::PipelineStageFlags2::CLEAR)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(
            vk::PipelineStageFlags2::FRAGMENT_SHADER
                | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        )
        .dst_access_mask(
            vk::AccessFlags2::SHADER_SAMPLED_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        )
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
    // SAFETY: These newly allocated images are pinned, unused, and initialized by the first submitted shadow command only.
    unsafe {
        device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&before),
        );
        device.cmd_clear_color_image(
            command,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [1.; 4] },
            &[color],
        );
        device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&after),
        );
    }
}

/// Shadow images contain one mip and one layer.
fn range(aspects: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspects)
        .level_count(1)
        .layer_count(1)
}
