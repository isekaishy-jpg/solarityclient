//! Dynamic-rendering command recording and binary-semaphore presentation.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;

use super::M2FrameContext;
use super::resource::M2FrameSlot;

/// Borrowed state required to record one acquired image.
pub(super) struct RecordContext<'a> {
    pub(super) device: &'a Device,
    pub(super) capture: Option<&'a FrameReadback>,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) depth_image: vk::Image,
    pub(super) depth_view: vk::ImageView,
    pub(super) extent: (u32, u32),
    pub(super) frame_sets: [vk::DescriptorSet; 3],
    pub(super) material_stride: vk::DeviceSize,
    pub(super) pipelines: &'a M2PipelineRegistry,
    pub(super) meshes: &'a M2MeshRegistry,
    pub(super) texture_sets: &'a M2TextureSetRegistry,
    pub(super) draws: &'a [M2PreparedDraw],
}

#[derive(Default)]
struct M2CommandBindings {
    pipeline: vk::Pipeline,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
}

/// Records attachment transitions, state binding, and every indexed draw.
pub(super) fn record_draws(context: RecordContext<'_>) -> Result<(), VulkanError> {
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The slot's primary buffer was reset and is not pending.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin_info)
    }
    .map_err(|source| VulkanError::operation("begin M2 frame command buffer", source))?;
    transition_attachments(&context);

    let color_clear = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 1.0],
        },
    };
    let depth_clear = vk::ClearValue {
        depth_stencil: vk::ClearDepthStencilValue {
            depth: 1.0,
            stencil: 0,
        },
    };
    let color_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(context.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(color_clear);
    let depth_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(context.depth_view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(depth_clear);
    let color_attachments = [color_attachment];
    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent: vk::Extent2D {
            width: context.extent.0,
            height: context.extent.1,
        },
    };
    let rendering_info = vk::RenderingInfo::default()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&color_attachments)
        .depth_attachment(&depth_attachment)
        .stencil_attachment(&depth_attachment);
    // SAFETY: Dynamic rendering is enabled and all attachment views/layouts
    // match the pipeline formats and transitions recorded above.
    unsafe {
        context
            .device
            .cmd_begin_rendering(context.command_buffer, &rendering_info)
    };

    let viewport = vk::Viewport {
        x: 0.0,
        y: context.extent.1 as f32,
        width: context.extent.0 as f32,
        height: -(context.extent.1 as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = render_area;
    // SAFETY: Both dynamic states were declared by every M2 pipeline.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[scissor]);
    }
    let mut bindings = M2CommandBindings::default();
    for (draw_index, draw) in context.draws.iter().copied().enumerate() {
        record_draw(&context, draw_index, draw, &mut bindings)?;
    }
    // SAFETY: A matching dynamic-rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    transition_to_present(&context);
    // SAFETY: Every referenced resource outlives slot fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end M2 frame command buffer", source))
}

/// Submits one completed slot and queues its acquired image for presentation.
pub(super) fn submit_and_present(
    context: &M2FrameContext<'_>,
    slot: &mut M2FrameSlot,
    present_semaphore: vk::Semaphore,
    image_index: u32,
) -> Result<(), VulkanError> {
    let wait = vk::SemaphoreSubmitInfo::default()
        .semaphore(slot.image_available())
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT);
    let command = vk::CommandBufferSubmitInfo::default().command_buffer(slot.command_buffer());
    let signal = vk::SemaphoreSubmitInfo::default()
        .semaphore(present_semaphore)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS);
    let waits = [wait];
    let commands = [command];
    let signals = [signal];
    let submit = vk::SubmitInfo2::default()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&commands)
        .signal_semaphore_infos(&signals);
    slot.reset_fence(context.device)?;
    // SAFETY: The command buffer is executable and all synchronization objects
    // belong to this slot and live through fence retirement.
    if let Err(source) = unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit], slot.fence())
    } {
        slot.restore_signaled_fence(context.device)?;
        return Err(VulkanError::operation("submit M2 frame", source));
    }

    let present_waits = [present_semaphore];
    let swapchains = [context.swapchain];
    let image_indices = [image_index];
    let present_info = vk::PresentInfoKHR::default()
        .wait_semaphores(&present_waits)
        .swapchains(&swapchains)
        .image_indices(&image_indices);
    // SAFETY: Presentation waits for this slot's render signal and uses the
    // exact image index returned by acquisition.
    unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present_info)
    }
    .map_err(|source| swapchain_error("present M2 frame", source))?;
    Ok(())
}

/// Binds one prepared packet and records its direct unsigned-short index range.
fn record_draw(
    context: &RecordContext<'_>,
    draw_index: usize,
    draw: M2PreparedDraw,
    bindings: &mut M2CommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownM2PipelineHandle)?;
    let (vertex_buffer, index_buffer) = context
        .meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownM2MeshHandle)?;
    let texture_set = context
        .texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let dynamic_offset = u64::try_from(draw_index)
        .ok()
        .and_then(|index| index.checked_mul(context.material_stride))
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or(VulkanError::M2FrameCapacity)?;
    let descriptor_sets = [
        context.frame_sets[0],
        context.frame_sets[1],
        context.frame_sets[2],
        texture_set,
    ];
    // SAFETY: Prepared draw validation joins compatible renderer-local handles;
    // the descriptor sets match the common layout and dynamic offset alignment.
    unsafe {
        if bindings.pipeline != pipeline {
            context.device.cmd_bind_pipeline(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
            bindings.pipeline = pipeline;
        }
        if bindings.vertex_buffer != vertex_buffer {
            context.device.cmd_bind_vertex_buffers(
                context.command_buffer,
                0,
                &[vertex_buffer],
                &[0],
            );
            bindings.vertex_buffer = vertex_buffer;
        }
        if bindings.index_buffer != index_buffer {
            context.device.cmd_bind_index_buffer(
                context.command_buffer,
                index_buffer,
                0,
                vk::IndexType::UINT16,
            );
            bindings.index_buffer = index_buffer;
        }
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &descriptor_sets,
            &[dynamic_offset],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &draw.push_constants().to_bytes(),
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            draw.index_count(),
            1,
            draw.first_index(),
            0,
            0,
        );
    }
    Ok(())
}

/// Discards prior attachment contents and exposes color/depth writes.
fn transition_attachments(context: &RecordContext<'_>) {
    let color_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let depth_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let barriers = [
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(context.image)
            .subresource_range(color_range),
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(
                vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            )
            .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .image(context.depth_image)
            .subresource_range(depth_range),
    ];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: Synchronization2 is enabled and both attachments' prior contents
    // are intentionally discarded before their first writes in this slot.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Exposes completed color writes to the presentation engine.
fn transition_to_present(context: &RecordContext<'_>) {
    if let Some(capture) = context.capture {
        capture.record(
            context.device,
            context.command_buffer,
            context.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        return;
    }
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(range);
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The barrier follows the rendering scope in this command buffer.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}
