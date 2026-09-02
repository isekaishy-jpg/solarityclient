//! Vulkan copy, linear blit, overlay, and asynchronous presentation commands.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_ui_frame::{UiOverlayRecordContext, record_loaded_overlay};

use super::resource::FrameSlot;
use super::{FrameContext, FrameUiContext, swapchain_error};

/// Borrowed resources needed to record one acquired swapchain image.
pub(super) struct RecordContext<'a> {
    pub(super) device: &'a Device,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) source_image: vk::Image,
    pub(super) source_buffer: vk::Buffer,
    pub(super) source_extent: (u32, u32),
    pub(super) source_end: vk::Offset3D,
    pub(super) source_initialized: bool,
    pub(super) upload_source: bool,
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) frame_extent: (u32, u32),
    pub(super) destination: [vk::Offset3D; 2],
    pub(super) ui: Option<FrameUiContext<'a>>,
}

/// Records a changed upload, aspect-fit linear blit, and optional UI overlay.
pub(super) fn record_frame(context: RecordContext<'_>) -> Result<(), VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The slot was fenced and its command pool was reset.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|source| VulkanError::operation("begin cinematic command buffer", source))?;

    if context.upload_source {
        transition_source_to_upload(&context);
        copy_source(&context);
        transition_source_to_blit(&context);
    }
    transition_target_to_blit(&context);
    clear_target(&context);
    blit_source(&context);
    if let Some(ui) = context.ui.filter(|ui| !ui.draws.is_empty()) {
        transition_target_to_ui(&context);
        record_loaded_overlay(UiOverlayRecordContext {
            device: context.device,
            command_buffer: context.command_buffer,
            image_view: context.image_view,
            extent: context.frame_extent,
            logical_extent: ui.logical_extent,
            pipelines: ui.pipelines,
            meshes: ui.meshes,
            texture_sets: ui.texture_sets,
            draws: ui.draws,
        })?;
        transition_ui_to_present(&context);
    } else {
        transition_blit_to_present(&context);
    }
    // SAFETY: Every referenced resource remains alive through fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end cinematic command buffer", source))
}

/// Makes the retained image writable, preserving its known prior layout.
fn transition_source_to_upload(context: &RecordContext<'_>) {
    let (source_stage, source_access, old_layout) = if context.source_initialized {
        (
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        )
    } else {
        (
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::ImageLayout::UNDEFINED,
        )
    };
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(source_stage)
        .src_access_mask(source_access)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(context.source_image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: All prior readers were fenced before a changed source upload.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Copies the exact persistently mapped RGBA8 buffer into its retained image.
fn copy_source(context: &RecordContext<'_>) {
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    let region = vk::BufferImageCopy::default()
        .image_subresource(layers)
        .image_extent(vk::Extent3D {
            width: context.source_extent.0,
            height: context.source_extent.1,
            depth: 1,
        });
    // SAFETY: Validation proves the source buffer contains this exact footprint.
    unsafe {
        context.device.cmd_copy_buffer_to_image(
            context.command_buffer,
            context.source_buffer,
            context.source_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        )
    };
}

/// Makes the completed source upload readable by the linear blit.
fn transition_source_to_blit(context: &RecordContext<'_>) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .image(context.source_image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The copy precedes this barrier in the same command buffer.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Discards prior swapchain contents and makes the acquired image writable.
fn transition_target_to_blit(context: &RecordContext<'_>) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(context.image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The acquired image is not used by another queue operation.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Clears letterbox or pillarbox space to opaque black.
fn clear_target(context: &RecordContext<'_>) {
    let clear = vk::ClearColorValue {
        float32: [0.0, 0.0, 0.0, 1.0],
    };
    // SAFETY: The complete target color subresource is transfer-writable.
    unsafe {
        context.device.cmd_clear_color_image(
            context.command_buffer,
            context.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &clear,
            &[color_range()],
        )
    };
}

/// Scales source RGBA into the aspect-fit destination with hardware filtering.
fn blit_source(context: &RecordContext<'_>) {
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    let region = vk::ImageBlit::default()
        .src_subresource(layers)
        .src_offsets([vk::Offset3D::default(), context.source_end])
        .dst_subresource(layers)
        .dst_offsets(context.destination);
    // SAFETY: Adapter validation requires RGBA8 linear blit source support and
    // swapchain-format blit destination support for these live subresources.
    unsafe {
        context.device.cmd_blit_image(
            context.command_buffer,
            context.source_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            context.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
            vk::Filter::LINEAR,
        )
    };
}

/// Makes the blit result available as an overlay color attachment.
fn transition_target_to_ui(context: &RecordContext<'_>) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        )
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .image(context.image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The blit precedes the overlay in this command buffer.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Exposes the completed overlay to presentation.
fn transition_ui_to_present(context: &RecordContext<'_>) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The dynamic-rendering scope has ended.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Exposes a blit result directly when no overlay is active.
fn transition_blit_to_present(context: &RecordContext<'_>) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The blit is complete before this same-buffer transition.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Submits asynchronously and queues the acquired image for FIFO presentation.
pub(super) fn submit_and_present(
    context: &FrameContext<'_>,
    slot: &mut FrameSlot,
    present_semaphore: vk::Semaphore,
    image_index: u32,
) -> Result<(), VulkanError> {
    let waits = [vk::SemaphoreSubmitInfo::default()
        .semaphore(slot.image_available())
        .stage_mask(vk::PipelineStageFlags2::TRANSFER)];
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
        return Err(VulkanError::operation("submit cinematic frame", source));
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
    .map_err(|source| swapchain_error("present cinematic frame", source))?;
    Ok(())
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
