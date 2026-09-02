//! Terrain dynamic-rendering commands and swapchain presentation.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;

use super::TerrainFrameContext;
use super::resource::TerrainFrameSlot;

pub(super) struct RecordContext<'a> {
    pub(super) device: &'a Device,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) depth_image: vk::Image,
    pub(super) depth_view: vk::ImageView,
    pub(super) extent: (u32, u32),
    pub(super) scene_set: vk::DescriptorSet,
    pub(super) pipelines: &'a TerrainPipelineRegistry,
    pub(super) meshes: &'a TerrainMeshRegistry,
    pub(super) texture_sets: &'a TerrainTextureSetRegistry,
    pub(super) draws: &'a [TerrainPreparedDraw],
}

pub(super) fn record_draws(context: RecordContext<'_>) -> Result<(), VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The slot command pool was reset and the buffer is not pending.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|source| VulkanError::operation("begin terrain command buffer", source))?;
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
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(context.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(color_clear);
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(context.depth_view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(depth_clear);
    let colors = [color];
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
        .color_attachments(&colors)
        .depth_attachment(&depth)
        .stencil_attachment(&depth);
    // SAFETY: Dynamic rendering is enabled and attachment layouts/formats agree.
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
    // SAFETY: Every terrain pipeline declares viewport and scissor dynamic.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[render_area]);
    }
    for draw in context.draws.iter().copied() {
        record_draw(&context, draw)?;
    }
    // SAFETY: A matching dynamic-rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    transition_to_present(&context);
    // SAFETY: Every referenced resource outlives slot fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end terrain command buffer", source))
}

fn record_draw(context: &RecordContext<'_>, draw: TerrainPreparedDraw) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownTerrainPipelineHandle)?;
    let (vertex_buffer, index_buffer) = context
        .meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownTerrainMeshHandle)?;
    let material_set = context
        .texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownTerrainTextureSetHandle)?;
    let sets = [context.scene_set, material_set];
    // SAFETY: Prepared packets already prove compatible local handles/ranges.
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
            vk::IndexType::UINT16,
        );
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets,
            &[],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            &draw.push_bytes(),
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

fn transition_attachments(context: &RecordContext<'_>) {
    let color_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let depth_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
        .level_count(1)
        .layer_count(1);
    let barriers = [
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(context.image)
            .subresource_range(color_range),
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
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
    // SAFETY: Synchronization2 is enabled and old attachment contents are discarded.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

fn transition_to_present(context: &RecordContext<'_>) {
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(range)];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: This barrier follows the completed rendering scope.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

pub(super) fn submit_and_present(
    context: &TerrainFrameContext<'_>,
    slot: &mut TerrainFrameSlot,
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
    // SAFETY: Command and synchronization objects remain live through the fence.
    if let Err(source) = unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit], slot.fence())
    } {
        slot.restore_signaled_fence(context.device)?;
        return Err(VulkanError::operation("submit terrain frame", source));
    }
    let waits = [present_semaphore];
    let swapchains = [context.swapchain];
    let indices = [image_index];
    let present = vk::PresentInfoKHR::default()
        .wait_semaphores(&waits)
        .swapchains(&swapchains)
        .image_indices(&indices);
    // SAFETY: Presentation waits for this frame's signal and acquired image.
    unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present)
    }
    .map_err(|source| swapchain_error("present terrain frame", source))?;
    Ok(())
}
