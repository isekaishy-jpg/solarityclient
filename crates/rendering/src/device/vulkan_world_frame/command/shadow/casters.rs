//! Shared M2 and WMO silhouettes for primary and environment maps.

#![allow(unsafe_code)]

use super::super::{RecordContext, WorldCommandBindings, dynamic_offset};
use crate::{M2PreparedDraw, M2ShadowMaterial, device::VulkanError};
use ash::vk;

/// Bit three selects primary scenery; bits zero through two select environment maps.
pub(super) fn record_scenery(
    context: &RecordContext<'_>,
    caster_set: vk::DescriptorSet,
    mask: u8,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(frame) = context.environment_frame else {
        return Ok(());
    };
    // 7BBC50 submits WMO groups before the skeletal caster queues.
    for (index, caster) in frame.wmo_casters().iter().enumerate() {
        if caster.maps & mask == 0 {
            continue;
        }
        let draw = caster.draw;
        let (pipeline, layout) = context
            .shadow_pipeline
            .wmo(caster.blend_mode == solarity_asset::WorldModelBlendMode::AlphaKey);
        let (vertex, indices) = context
            .world_model_meshes
            .buffers(draw.mesh())
            .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
        let texture = context
            .world_model_texture_sets
            .raw(draw.texture_set())
            .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
        let offset = dynamic_offset(
            context.world_model_draws.len() + index,
            context.world_model_material_stride,
        )?;
        let range = draw.index_range();
        // SAFETY: The validated logical packet shares the world material and texture ABI.
        unsafe {
            bindings.bind_pipeline(context, pipeline);
            bindings.bind_vertex(context, (vertex, 0));
            bindings.bind_index(context, (indices, 0), vk::IndexType::UINT32);
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[caster_set, context.frame_sets[2], texture],
                &[offset],
            );
            context
                .device
                .cmd_draw_indexed(context.command_buffer, range[1], 1, range[0], 0, 0);
        }
    }
    let material_base = context.m2_draws.len()
        + context.sky_models.map_or(0, |models| models.draw_count())
        + context
            .shadow_frame
            .map_or(0, |frame| frame.casters().len());
    let casters = frame.m2_casters();
    let mut index = 0;
    while let Some(caster) = casters.get(index) {
        if caster.maps & mask == 0 {
            index += 1;
            continue;
        }
        let count = 1 + casters[index + 1..]
            .iter()
            .take_while(|next| {
                next.maps & mask != 0 && caster.draw.can_instance_shadow_with(next.draw)
            })
            .count();
        record_m2_instances(
            context,
            material_base + index,
            caster.draw,
            u32::try_from(count).map_err(|_| VulkanError::WorldFrameCapacity)?,
            caster_set,
            bindings,
        )?;
        index += count;
    }
    Ok(())
}

pub(super) fn record_m2(
    context: &RecordContext<'_>,
    material_index: usize,
    draw: M2PreparedDraw,
    caster_set: vk::DescriptorSet,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    record_m2_instances(context, material_index, draw, 1, caster_set, bindings)
}

/// One indexed submission reads consecutive instance records from the frame stream.
fn record_m2_instances(
    context: &RecordContext<'_>,
    material_index: usize,
    draw: M2PreparedDraw,
    instance_count: u32,
    caster_set: vk::DescriptorSet,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(material) = draw.shadow_material() else {
        return Ok(());
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
    let first_instance =
        u32::try_from(material_index).map_err(|_| VulkanError::WorldFrameCapacity)?;
    let sets = [
        caster_set,
        context.frame_sets[6],
        context.frame_sets[7],
        texture,
    ];
    let layout = context.shadow_pipeline.layout();
    // SAFETY: Prepared packets and the frame's palette/range validation establish each resource join.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, (vertex, 0));
        bindings.bind_index(context, (indices, 0), vk::IndexType::UINT16);
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets,
            &[0],
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
            instance_count,
            draw.first_index(),
            0,
            first_instance,
        );
    }
    Ok(())
}
