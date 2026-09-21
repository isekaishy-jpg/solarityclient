//! Small ordered compositor layers surrounding the parallel world command ranges.

#![allow(unsafe_code)]

use super::super::recording::scene::SceneCommand;
use super::{RecordContext, WorldCommandBindings};
use crate::VulkanError;
use ash::vk;
/// The sky occupies the reserved far depth range.
pub(super) fn record_sky(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.sky_frame else {
        return;
    };
    let (pipeline, layout) = context.sky_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.sky_resources.vertex_buffer());
    bindings.bind_index(
        context,
        context.sky_resources.index_buffer(),
        vk::IndexType::UINT16,
    );
    let matrix = frame.view_projection().to_cols_array();
    let mut pushes = [0_u8; 64];
    for (word, value) in pushes.as_chunks_mut::<4>().0.iter_mut().zip(matrix) {
        *word = value.to_le_bytes();
    }
    // SAFETY: The retired slot owns all 122 vertices and six 50-index strips;
    // the prepared pipeline has the matching 64-byte push range and no sampled inputs.
    unsafe {
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &pushes,
        );
        context
            .device
            .cmd_draw_indexed(context.command_buffer, 300, 1, 0, 0, 0);
    }
}

pub(super) fn record_celestials(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.celestial_frame else {
        return;
    };
    let (pipeline, layout) = context.celestial_pipeline.raw();
    for (draw, resources) in frame.draws().into_iter().zip(context.celestial_resources) {
        if draw.mesh().indices().is_empty() {
            continue;
        }
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, resources.vertex_buffer());
        bindings.bind_index(context, resources.index_buffer(), vk::IndexType::UINT16);
        let mut pushes = [0_u8; 64];
        for (word, value) in pushes
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(draw.view_projection().to_cols_array())
        {
            *word = value.to_le_bytes();
        }
        // SAFETY: Slot retirement precedes the six-vertex write and descriptor update;
        // the matching PCT pipeline consumes the complete 64-byte matrix range.
        unsafe {
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[resources.descriptor()],
                &[],
            );
            context.device.cmd_push_constants(
                context.command_buffer,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &pushes,
            );
            context.device.cmd_draw_indexed(
                context.command_buffer,
                draw.mesh().indices().len() as u32,
                1,
                0,
                0,
                0,
            );
        }
    }
}

pub(super) fn record_clouds(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.cloud_frame else {
        return;
    };
    let (pipeline, layout) = context.cloud_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.cloud_resources.vertex_buffer());
    bindings.bind_index(
        context,
        context.cloud_resources.index_buffer(),
        vk::IndexType::UINT16,
    );
    let matrix = frame.view_projection().to_cols_array();
    let mut pushes = [0_u8; 64];
    for (word, value) in pushes.as_chunks_mut::<4>().0.iter_mut().zip(matrix) {
        *word = value.to_le_bytes();
    }
    // SAFETY: The retired slot owns all 177 vertices and one 374-index strip;
    // the prepared pipeline has the matching 64-byte push range and a resident procedural texture.
    unsafe {
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[context.cloud_resources.descriptor()],
            &[],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &pushes,
        );
        context
            .device
            .cmd_draw_indexed(context.command_buffer, 374, 1, 0, 0, 0);
    }
}

pub(super) fn record_ripples(
    context: &RecordContext<'_>,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(frame) = context.ripple_frame.filter(|frame| frame.draw_count() != 0) else {
        return Ok(());
    };
    let (pipeline, layout) = context.ripple_pipeline.raw();
    let sets = context.ripple_resources.sets();
    let mut offset = 0;
    for (index, pass) in frame.passes().into_iter().enumerate() {
        let Some(pass) = pass else {
            continue;
        };
        let count = pass.draw_vertex_count();
        if count != 0 {
            bindings.draw(
                context,
                SceneCommand {
                    pipeline,
                    layout,
                    vertex: (context.ripple_resources.buffer(), offset),
                    count,
                    ..Default::default()
                }
                .parameters(
                    &[sets[index]],
                    &[],
                    &frame.push_bytes(),
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                ),
            )?;
        }
        offset += (pass.vertices().len() * crate::WaterRippleRenderVertex::BYTE_SIZE) as u64;
    }
    Ok(())
}

/// Native 77F9D0 draws camera-relative billboards after the world effect queues.
pub(super) fn record_underwater(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context
        .underwater_frame
        .filter(|frame| frame.draw_count() != 0)
    else {
        return;
    };
    let (pipeline, layout) = context.underwater_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.underwater_resources.vertex_buffer());
    let (buffer, offset) = context.underwater_resources.index_buffer();
    bindings.bind_index(context, (buffer, offset), vk::IndexType::UINT16);
    // SAFETY: Retired-slot writes validate both complete geometry banks and this
    // texture descriptor; pipeline preparation fixes the 96-byte push ABI.
    unsafe {
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &frame.push_bytes(),
        );
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[context.underwater_resources.descriptor()],
            &[],
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            frame.indices().len() as u32,
            1,
            0,
            0,
            0,
        );
    }
}
