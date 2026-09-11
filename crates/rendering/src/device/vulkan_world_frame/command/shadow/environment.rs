//! Ordered cache clears and partial environment-map rendering.

#![allow(unsafe_code)]

use ash::vk;

use super::super::RecordContext;
use super::{casters, range};
use crate::device::VulkanError;

pub(super) fn record(context: &RecordContext<'_>) -> Result<(), VulkanError> {
    let Some(frame) = context.environment_frame else {
        return Ok(());
    };
    if !context.environment_images.initialized() {
        initialize(context);
    }
    for (index, pass) in frame.passes().into_iter().enumerate() {
        let Some(pass) = pass else {
            continue;
        };
        let update = pass.update();
        let (color_image, color_view) = context.environment_images.color(index, update.buffer);
        let (depth_image, depth_view) = context.environment_images.depth();
        let [x, y, width, height] = update.pixel_viewport(pass.projection().texture_size());
        let rect = vk::Rect2D {
            offset: vk::Offset2D {
                x: x as i32,
                y: y as i32,
            },
            extent: vk::Extent2D { width, height },
        };
        let barriers = [
            vk::ImageMemoryBarrier2::default()
                .image(color_image)
                .subresource_range(range(vk::ImageAspectFlags::COLOR))
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_stage_mask(
                    vk::PipelineStageFlags2::FRAGMENT_SHADER
                        | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                )
                .src_access_mask(
                    vk::AccessFlags2::SHADER_SAMPLED_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                )
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(
                    vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                )
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED),
            vk::ImageMemoryBarrier2::default()
                .image(depth_image)
                .subresource_range(range(
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                ))
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_stage_mask(
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                )
                .src_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_stage_mask(
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                )
                .dst_access_mask(
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED),
        ];
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(color_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [1.; 4] },
            })];
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
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
            .render_area(rect)
            .layer_count(1)
            .color_attachments(&color)
            .depth_attachment(&depth)
            .stencil_attachment(&depth);
        let viewport = vk::Viewport {
            x: x as f32,
            y: (y + height) as f32,
            width: width as f32,
            height: -(height as f32),
            min_depth: 0.,
            max_depth: 1.,
        };
        // SAFETY: One graphics queue orders reads from the preceding frame before
        // this region's writes. CLEAR affects only render_area, retaining other regions.
        unsafe {
            context.device.cmd_pipeline_barrier2(
                context.command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&barriers),
            );
            context
                .device
                .cmd_begin_rendering(context.command_buffer, &rendering);
            context
                .device
                .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
            context
                .device
                .cmd_set_scissor(context.command_buffer, 0, &[rect]);
        }
        casters::record_scenery(
            context,
            context.shadow_resources.environment_caster_set(index),
            1 << index,
        )?;
        let barrier = [vk::ImageMemoryBarrier2::default()
            .image(color_image)
            .subresource_range(range(vk::ImageAspectFlags::COLOR))
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
        // SAFETY: Receiver descriptors publish this image only after the final region.
        unsafe {
            context.device.cmd_end_rendering(context.command_buffer);
            context.device.cmd_pipeline_barrier2(
                context.command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&barrier),
            );
        }
    }
    Ok(())
}

fn initialize(context: &RecordContext<'_>) {
    let color_range = range(vk::ImageAspectFlags::COLOR);
    for image in context.environment_images.color_images() {
        let before = [vk::ImageMemoryBarrier2::default()
            .image(image)
            .subresource_range(color_range)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .dst_stage_mask(vk::PipelineStageFlags2::CLEAR)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
        let after = [vk::ImageMemoryBarrier2::default()
            .image(image)
            .subresource_range(color_range)
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
        // SAFETY: Every new cache image is cleared to native visibility one before
        // any frame can sample it, including maps whose first region is deferred.
        unsafe {
            context.device.cmd_pipeline_barrier2(
                context.command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&before),
            );
            context.device.cmd_clear_color_image(
                context.command_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [1.; 4] },
                &[color_range],
            );
            context.device.cmd_pipeline_barrier2(
                context.command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&after),
            );
        }
    }
}
