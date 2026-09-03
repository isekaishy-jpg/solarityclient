//! Dynamic-rendering commands and binary-semaphore swapchain presentation.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;

use super::UiFrameContext;
use super::resource::UiFrameSlot;

const UPDATE_CHUNK_SIZE: usize = 65_536;

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
    pub(super) overlay: &'a [UiPreparedDraw],
}

/// UI state appended after another pass left the swapchain color attachment live.
#[derive(Clone, Copy)]
pub(in crate::device) struct UiOverlayRecordContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) command_buffer: vk::CommandBuffer,
    pub(in crate::device) image_view: vk::ImageView,
    pub(in crate::device) extent: (u32, u32),
    pub(in crate::device) logical_extent: [f32; 2],
    pub(in crate::device) pipelines: &'a UiPipelineRegistry,
    pub(in crate::device) meshes: &'a UiMeshRegistry,
    pub(in crate::device) texture_sets: &'a UiTextureSetRegistry,
    pub(in crate::device) draws: &'a [UiPreparedDraw],
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
    record_mesh_updates(
        context.device,
        context.command_buffer,
        context.meshes,
        context.draws,
    )?;
    record_mesh_updates(
        context.device,
        context.command_buffer,
        context.meshes,
        context.overlay,
    )?;
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
    for draw in context.draws.iter().copied() {
        record_draw(
            context.device,
            context.command_buffer,
            context.pipelines,
            context.meshes,
            context.texture_sets,
            draw,
            context.logical_extent,
            context.extent,
        )?;
    }
    for draw in context.overlay.iter().copied() {
        record_draw(
            context.device,
            context.command_buffer,
            context.pipelines,
            context.meshes,
            context.texture_sets,
            draw,
            context.logical_extent,
            context.extent,
        )?;
    }
    // SAFETY: A matching dynamic-rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    transition_to_present(&context);
    // SAFETY: Every referenced resource outlives fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end UI frame command buffer", source))
}

/// Appends UI draws without clearing the color produced by a Glue model pass.
pub(in crate::device) fn record_loaded_overlay(
    context: UiOverlayRecordContext<'_>,
) -> Result<(), VulkanError> {
    if context.draws.is_empty() {
        return Ok(());
    }
    record_mesh_updates(
        context.device,
        context.command_buffer,
        context.meshes,
        context.draws,
    )?;
    let attachment = vk::RenderingAttachmentInfo::default()
        .image_view(context.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE);
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
    // SAFETY: The preceding model scope ended with color attachment layout
    // retained; UI pipelines use an independent color-only rendering scope.
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
    for draw in context.draws.iter().copied() {
        record_draw(
            context.device,
            context.command_buffer,
            context.pipelines,
            context.meshes,
            context.texture_sets,
            draw,
            context.logical_extent,
            context.extent,
        )?;
    }
    // SAFETY: A matching color-only dynamic-rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    Ok(())
}

/// Embeds each changed retained mesh once before the rendering scope begins.
fn record_mesh_updates(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    meshes: &UiMeshRegistry,
    draws: &[UiPreparedDraw],
) -> Result<(), VulkanError> {
    for draw in draws {
        let mesh = draw.mesh();
        let updates = meshes.take_pending_updates(mesh)?;
        if let Some((buffer, offset, bytes)) = updates.vertex {
            record_buffer_update(
                device,
                command_buffer,
                buffer,
                offset,
                bytes,
                vk::PipelineStageFlags2::VERTEX_INPUT,
                vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
            );
        }
        if let Some((buffer, offset, bytes)) = updates.index {
            record_buffer_update(
                device,
                command_buffer,
                buffer,
                offset,
                bytes,
                vk::PipelineStageFlags2::INDEX_INPUT,
                vk::AccessFlags2::INDEX_READ,
            );
        }
    }
    Ok(())
}

/// Writes one retained payload in Vulkan's bounded inline-update chunks.
fn record_buffer_update(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    buffer: vk::Buffer,
    buffer_offset: vk::DeviceSize,
    bytes: &[u8],
    destination_stage: vk::PipelineStageFlags2,
    destination_access: vk::AccessFlags2,
) {
    let prior_read_barriers = [vk::BufferMemoryBarrier2::default()
        .src_stage_mask(destination_stage)
        .src_access_mask(destination_access)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .buffer(buffer)
        .offset(buffer_offset)
        .size(bytes.len() as vk::DeviceSize)];
    let prior_read_dependency =
        vk::DependencyInfo::default().buffer_memory_barriers(&prior_read_barriers);
    // SAFETY: Queue submission order puts every prior frame in the first
    // synchronization scope, preventing this write from racing its UI reads.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &prior_read_dependency) };
    for (chunk_index, chunk) in bytes.chunks(UPDATE_CHUNK_SIZE).enumerate() {
        let offset = buffer_offset + (chunk_index * UPDATE_CHUNK_SIZE) as vk::DeviceSize;
        // SAFETY: Retained allocations cover every four-byte-aligned chunk and
        // Vulkan copies the supplied bytes into command-buffer-owned storage.
        unsafe { device.cmd_update_buffer(command_buffer, buffer, offset, chunk) };
    }
    let current_read_barriers = [vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(destination_stage)
        .dst_access_mask(destination_access)
        .buffer(buffer)
        .offset(buffer_offset)
        .size(bytes.len() as vk::DeviceSize)];
    let current_read_dependency =
        vk::DependencyInfo::default().buffer_memory_barriers(&current_read_barriers);
    // SAFETY: The transfer commands precede all UI input reads in this buffer.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &current_read_dependency) };
}

/// Binds one validated packet and records its unsigned-32 indexed range.
#[allow(clippy::too_many_arguments)]
fn record_draw(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipelines: &UiPipelineRegistry,
    meshes: &UiMeshRegistry,
    texture_sets: &UiTextureSetRegistry,
    draw: UiPreparedDraw,
    logical_extent: [f32; 2],
    physical_extent: (u32, u32),
) -> Result<(), VulkanError> {
    let (pipeline, layout) = pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownUiPipelineHandle)?;
    let (vertex_buffer, index_buffer) = meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownUiMeshHandle)?;
    // SAFETY: Prepared draws join compatible local handles and exact ranges.
    unsafe {
        let scissor = logical_scissor(draw.clip(), logical_extent, physical_extent);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_vertex_buffers(command_buffer, 0, &[vertex_buffer], &[0]);
        device.cmd_bind_index_buffer(command_buffer, index_buffer, 0, vk::IndexType::UINT32);
        if let Some(texture_set) = draw.texture_set() {
            let descriptor = texture_sets
                .raw(texture_set)
                .ok_or(VulkanError::UnknownUiTextureSetHandle)?;
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[descriptor],
                &[],
            );
        }
        device.cmd_push_constants(
            command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            &draw_state_bytes(logical_extent, draw.translation()),
        );
        // Every UI quad has the same six-index pattern. Reuse the mesh's
        // canonical prefix for each batch and select its contiguous vertices
        // through baseVertex, retaining the exact source draw order.
        device.cmd_draw_indexed(
            command_buffer,
            draw.index_count(),
            1,
            0,
            draw.base_vertex(),
            0,
        );
    }
    Ok(())
}

/// Serializes the four-float canvas and retained-translation push constant.
fn draw_state_bytes(extent: [f32; 2], translation: [f32; 2]) -> [u8; 16] {
    let mut bytes = [0; 16];
    for (index, value) in extent.into_iter().chain(translation).enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Converts stock bottom-left logical clipping into Vulkan framebuffer pixels.
fn logical_scissor(
    clip: Option<[f32; 4]>,
    logical_extent: [f32; 2],
    physical_extent: (u32, u32),
) -> vk::Rect2D {
    let Some([left, bottom, right, top]) = clip else {
        return vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: physical_extent.0,
                height: physical_extent.1,
            },
        };
    };
    let scale_x = physical_extent.0 as f32 / logical_extent[0];
    let scale_y = physical_extent.1 as f32 / logical_extent[1];
    let x0 = (left * scale_x)
        .floor()
        .clamp(0.0, physical_extent.0 as f32) as u32;
    let x1 = (right * scale_x)
        .ceil()
        .clamp(0.0, physical_extent.0 as f32) as u32;
    let y0 = ((logical_extent[1] - top) * scale_y)
        .floor()
        .clamp(0.0, physical_extent.1 as f32) as u32;
    let y1 = ((logical_extent[1] - bottom) * scale_y)
        .ceil()
        .clamp(0.0, physical_extent.1 as f32) as u32;
    vk::Rect2D {
        offset: vk::Offset2D {
            x: x0 as i32,
            y: y0 as i32,
        },
        extent: vk::Extent2D {
            width: x1.saturating_sub(x0),
            height: y1.saturating_sub(y0),
        },
    }
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
    .map_err(|source| swapchain_error("present UI frame", source))?;
    Ok(())
}
