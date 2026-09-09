//! Records the primary caster pass and its write-to-sample dependency.

#![allow(unsafe_code)]

use super::{RecordContext, dynamic_offset};
use crate::{M2ShadowMaterial, device::VulkanError};
use ash::vk;

/// Clears every active primary map, including frames where all casters disappeared.
pub(super) fn record_primary(context: &RecordContext<'_>) -> Result<(), VulkanError> {
    let Some(frame) = context.shadow_frame else {
        return Ok(());
    };
    let resources = context.shadow_resources;
    let size = resources.size();
    let color_range = range(vk::ImageAspectFlags::COLOR);
    let depth_range = range(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL);
    let barriers = [
        vk::ImageMemoryBarrier2::default()
            .image(resources.color_image())
            .subresource_range(color_range)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED),
        vk::ImageMemoryBarrier2::default()
            .image(resources.depth_image())
            .subresource_range(depth_range)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
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
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    let color = [vk::RenderingAttachmentInfo::default()
        .image_view(resources.color_view())
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue { float32: [1.0; 4] },
        })];
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(resources.depth_view())
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        });
    let rect = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: size,
            height: size,
        },
    };
    let rendering = vk::RenderingInfo::default()
        .render_area(rect)
        .layer_count(1)
        .color_attachments(&color)
        .depth_attachment(&depth)
        .stencil_attachment(&depth);
    let viewport = vk::Viewport {
        x: 0.0,
        y: size as f32,
        width: size as f32,
        height: -(size as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    // SAFETY: This slot's fence retired before map reuse; all attachments remain alive through submission.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency);
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
    let material_base =
        context.m2_draws.len() + context.sky_models.map_or(0, |models| models.draw_count());
    for (index, draw) in frame.casters().iter().copied().enumerate() {
        let Some(material) = draw.shadow_material() else {
            continue;
        };
        let info = context
            .m2_pipelines
            .info(draw.pipeline())
            .ok_or(VulkanError::UnknownM2PipelineHandle)?;
        let bone_class = info.permutation().vertex_index() / 10 % 3;
        let pipeline = context
            .shadow_pipeline
            .raw(bone_class, material == M2ShadowMaterial::AlphaTest)
            .ok_or(VulkanError::M2ShadowResourcesUnavailable)?;
        let (vertex, indices) = context
            .m2_meshes
            .buffers(draw.mesh())
            .ok_or(VulkanError::UnknownM2MeshHandle)?;
        let texture = context
            .m2_texture_sets
            .raw(draw.texture_set())
            .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
        let offset = dynamic_offset(material_base + index, context.m2_material_stride)?;
        let sets = [
            resources.caster_set(),
            context.frame_sets[6],
            context.frame_sets[7],
            texture,
        ];
        let layout = context.shadow_pipeline.layout();
        // SAFETY: Prepared packets and the frame's palette/range validation establish each resource join.
        unsafe {
            context.device.cmd_bind_pipeline(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
            context
                .device
                .cmd_bind_vertex_buffers(context.command_buffer, 0, &[vertex], &[0]);
            context.device.cmd_bind_index_buffer(
                context.command_buffer,
                indices,
                0,
                vk::IndexType::UINT16,
            );
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &sets,
                &[offset],
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
    }
    let barrier = [vk::ImageMemoryBarrier2::default()
        .image(resources.color_image())
        .subresource_range(color_range)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barrier);
    // SAFETY: End this attachment scope before exposing its color writes to the terrain fragment stage.
    unsafe {
        context.device.cmd_end_rendering(context.command_buffer);
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency);
    }
    Ok(())
}

/// Every shadow attachment contains exactly one mip and layer.
fn range(aspects: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspects)
        .level_count(1)
        .layer_count(1)
}
