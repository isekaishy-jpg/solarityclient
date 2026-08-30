//! Dynamic-rendering commands and binary-semaphore swapchain presentation.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;

use super::UiFrameContext;
use super::resource::UiFrameSlot;

/// Borrowed objects needed to record one acquired swapchain image.
pub(super) struct RecordContext<'a> {
    pub(super) device: &'a Device,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) extent: (u32, u32),
    pub(super) logical_extent: [f32; 2],
    pub(super) pipelines: &'a UiPipelineRegistry,
    pub(super) meshes: &'a UiMeshRegistry,
    pub(super) texture_sets: &'a UiTextureSetRegistry,
    pub(super) draws: &'a [UiPreparedDraw],
}

/// Records attachment transitions, shared state, and ordered indexed draws.
pub(super) fn record_draws(context: RecordContext<'_>) -> Result<(), VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The slot command pool was reset and this buffer is not pending.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|source| VulkanError::operation("begin UI frame command buffer", source))?;
    transition_to_color(&context);
    let clear = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 1.0],
        },
    };
    let attachment = vk::RenderingAttachmentInfo::default()
        .image_view(context.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear);
    let attachments = [attachment];
    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent: vk::Extent2D {
            width: context.extent.0,
            height: context.extent.1,
        },
    };
    let rendering = vk::RenderingInfo::default()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&attachments);
    // SAFETY: Dynamic rendering is enabled and the color view/layout agree.
    unsafe {
        context
            .device
            .cmd_begin_rendering(context.command_buffer, &rendering)
    };
    let viewport = vk::Viewport {
        x: 0.0,
        y: context.extent.1 as f32,
        width: context.extent.0 as f32,
        height: -(context.extent.1 as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    // SAFETY: Every UI pipeline declares viewport and scissor dynamic.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[render_area]);
    }
    let logical_extent_bytes = logical_extent_bytes(context.logical_extent);
    for draw in context.draws.iter().copied() {
        record_draw(&context, draw, &logical_extent_bytes)?;
    }
    // SAFETY: A matching dynamic-rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    transition_to_present(&context);
    // SAFETY: Every referenced resource outlives fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end UI frame command buffer", source))
}

/// Binds one validated packet and records its unsigned-32 indexed range.
fn record_draw(
    context: &RecordContext<'_>,
    draw: UiPreparedDraw,
    logical_extent_bytes: &[u8; 8],
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownUiPipelineHandle)?;
    let (vertex_buffer, index_buffer) = context
        .meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownUiMeshHandle)?;
    // SAFETY: Prepared draws join compatible local handles and exact ranges.
    unsafe {
        context.device.cmd_bind_pipeline(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline,
        );
        context
            .device
            .cmd_bind_vertex_buffers(context.command_buffer, 0, &[vertex_buffer], &[0]);
        context.device.cmd_bind_index_buffer(
            context.command_buffer,
            index_buffer,
            0,
            vk::IndexType::UINT32,
        );
        if let Some(texture_set) = draw.texture_set() {
            let descriptor = context
                .texture_sets
                .raw(texture_set)
                .ok_or(VulkanError::UnknownUiTextureSetHandle)?;
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[descriptor],
                &[],
            );
        }
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            logical_extent_bytes,
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

/// Serializes the two-float push constant once for the complete frame.
fn logical_extent_bytes(extent: [f32; 2]) -> [u8; 8] {
    let width = extent[0].to_le_bytes();
    let height = extent[1].to_le_bytes();
    [
        width[0], width[1], width[2], width[3], height[0], height[1], height[2], height[3],
    ]
}

/// Discards prior contents and exposes swapchain color writes.
fn transition_to_color(context: &RecordContext<'_>) {
    let range = color_range();
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .image(context.image)
        .subresource_range(range);
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: Synchronization2 is enabled and old contents are discarded.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Exposes completed color writes to the presentation engine.
fn transition_to_present(context: &RecordContext<'_>) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(color_range());
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: This barrier follows all rendering commands in the same buffer.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

const fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// Submits one completed slot and queues its acquired image for presentation.
pub(super) fn submit_and_present(
    context: &UiFrameContext<'_>,
    slot: &mut UiFrameSlot,
    present_semaphore: vk::Semaphore,
    image_index: u32,
) -> Result<(), VulkanError> {
    let waits = [vk::SemaphoreSubmitInfo::default()
        .semaphore(slot.image_available())
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
    let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(slot.command_buffer())];
    let signals = [vk::SemaphoreSubmitInfo::default()
        .semaphore(present_semaphore)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
    let submit = vk::SubmitInfo2::default()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&commands)
        .signal_semaphore_infos(&signals);
    slot.reset_fence(context.device)?;
    // SAFETY: Commands and synchronization objects remain live through the fence.
    if let Err(source) = unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit], slot.fence())
    } {
        slot.restore_signaled_fence(context.device)?;
        return Err(VulkanError::operation("submit UI frame", source));
    }
    let present_waits = [present_semaphore];
    let swapchains = [context.swapchain];
    let indices = [image_index];
    let present = vk::PresentInfoKHR::default()
        .wait_semaphores(&present_waits)
        .swapchains(&swapchains)
        .image_indices(&indices);
    // SAFETY: Presentation waits for this frame's signal and acquired image.
    unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present)
    }
    .map_err(|source| VulkanError::operation("present UI frame", source))?;
    Ok(())
}
