//! Validated resource lookup and compact draw capture; no worker borrows renderer registries.

use super::super::recording::scene::SceneCommand;
use super::order::LiquidQueue;
use super::{RecordContext, WorldCommandBindings};
use crate::VulkanError;
use crate::device::vulkan_liquid::LiquidDrawMaterial;
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_particle_draw::M2ParticlePreparedDraw;
use crate::device::vulkan_m2_ribbon_draw::M2RibbonPreparedDraw;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use ash::vk;
pub(super) fn effect_scene(
    context: &RecordContext<'_>,
    scene_index: Option<u32>,
    light_bank: crate::M2SceneLightBank,
) -> Result<crate::M2SceneUniform, VulkanError> {
    scene_index.map_or_else(
        || Ok(context.scene.m2(light_bank)),
        |index| {
            context
                .scene
                .m2_instance_scenes()
                .get(index as usize)
                .copied()
                .ok_or(VulkanError::WorldFrameCapacity)
        },
    )
}

pub(super) fn record_particle(
    context: &RecordContext<'_>,
    draw: M2ParticlePreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .m2_particle_pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownM2ParticlePipelineHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let (scene_set, scene_offset) = instance_scene(
        context,
        draw.scene_index(),
        m2_scene_set(context, draw.light_bank()),
    )?;
    let sets = [scene_set, texture];
    let scene = effect_scene(context, draw.scene_index(), draw.light_bank())?;
    let material = context
        .m2_particle_pipelines
        .info(draw.pipeline())
        .ok_or(VulkanError::UnknownM2ParticlePipelineHandle)?
        .material();
    if scene.fog_enabled() {
        bindings.fog.publish(
            scene.fog_parameters(),
            material.fog_mode(),
            scene.fog_color(),
        );
    }
    bindings.draw(
        context,
        SceneCommand {
            pipeline,
            layout,
            vertex: context.particle_vertex_buffer,
            index: Some((
                context.particle_index_buffer.0,
                context.particle_index_buffer.1,
                vk::IndexType::UINT32,
            )),
            count: draw.index_count(),
            first: draw.first_index(),
            vertex_offset: draw.vertex_offset(),
            ..Default::default()
        }
        .parameters(
            &sets,
            &[scene_offset],
            &draw.alpha_reference().to_le_bytes(),
            vk::ShaderStageFlags::FRAGMENT,
        ),
    )
}

pub(super) fn record_ribbon(
    context: &RecordContext<'_>,
    draw: M2RibbonPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .m2_ribbon_pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownM2RibbonPipelineHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let (scene_set, scene_offset) = instance_scene(
        context,
        draw.scene_index(),
        m2_scene_set(context, draw.light_bank()),
    )?;
    let sets = [scene_set, texture];
    let scene = effect_scene(context, draw.scene_index(), draw.light_bank())?;
    let material = context
        .m2_ribbon_pipelines
        .info(draw.pipeline())
        .ok_or(VulkanError::UnknownM2RibbonPipelineHandle)?
        .material();
    if draw.first_material_pass() && scene.fog_enabled() {
        bindings.fog.publish(
            scene.fog_parameters(),
            material.fog_mode(),
            scene.fog_color(),
        );
    }
    let fog = bindings.fog.push_bytes(!material.is_unfogged());
    bindings.draw(
        context,
        SceneCommand {
            pipeline,
            layout,
            vertex: context.ribbon_vertex_buffer,
            count: draw.vertex_count(),
            first: draw.first_vertex(),
            ..Default::default()
        }
        .parameters(
            &sets,
            &[scene_offset],
            &fog,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        ),
    )
}

pub(super) fn record_terrain(
    context: &RecordContext<'_>,
    draw: TerrainPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = if context.shadow_frame.is_some() {
        context
            .terrain_pipelines
            .raw_primary_shadow(draw.pipeline())
    } else {
        context.terrain_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownTerrainPipelineHandle)?;
    let (vertex, index) = context
        .terrain_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownTerrainMeshHandle)?;
    let texture = context
        .terrain_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownTerrainTextureSetHandle)?;
    let sets = [context.frame_sets[0], texture];
    let sets = [sets[0], sets[1], context.shadow_resources.receiver_set()];
    bindings.draw(
        context,
        SceneCommand {
            pipeline,
            layout,
            vertex: (vertex, 0),
            index: Some((index, 0, vk::IndexType::UINT16)),
            count: draw.index_count(),
            first: draw.first_index(),
            ..Default::default()
        }
        .parameters(
            &sets[..if context.shadow_frame.is_some() { 3 } else { 2 }],
            &[],
            &draw.push_bytes(),
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        ),
    )
}

pub(super) fn record_world_model(
    context: &RecordContext<'_>,
    draw_index: usize,
    draw: WorldModelPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    if let Some(color) = draw.submission_fog_color() {
        bindings.fog.publish(
            context.scene.world_model().fog_parameters(),
            crate::M2FogMode::SceneColor,
            color,
        );
    }
    let receives_shadow = context.shadow_frame.is_some();
    let (pipeline, layout) = if receives_shadow {
        context
            .world_model_pipelines
            .raw_primary_shadow(draw.pipeline())
    } else {
        context.world_model_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownWorldModelPipelineHandle)?;
    let (vertex, index) = context
        .world_model_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
    let texture = context
        .world_model_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
    let dynamic_offset = dynamic_offset(draw_index, context.world_model_material_stride)?;
    let sets = [
        context.frame_sets[1],
        context.frame_sets[2],
        texture,
        context.shadow_resources.receiver_set(),
    ];
    let sets = &sets[..if receives_shadow { 4 } else { 3 }];
    let range = draw.index_range();
    bindings.draw(
        context,
        SceneCommand {
            pipeline,
            layout,
            vertex: (vertex, 0),
            index: Some((index, 0, vk::IndexType::UINT32)),
            count: range[1],
            first: range[0],
            ..Default::default()
        }
        .parameters(sets, &[dynamic_offset], &[], vk::ShaderStageFlags::empty()),
    )
}

pub(super) fn record_m2(
    context: &RecordContext<'_>,
    draw_index: usize,
    draw: M2PreparedDraw,
    instance_count: u32,
    scene_set: vk::DescriptorSet,
    scene: crate::M2SceneUniform,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let material = context
        .m2_pipelines
        .info(draw.pipeline())
        .ok_or(VulkanError::UnknownM2PipelineHandle)?
        .material();
    if scene.fog_enabled() {
        bindings.fog.publish(
            scene.fog_parameters(),
            material.fog_mode(),
            draw.material().fog_color(),
        );
    }
    // Sky model packets follow the ordinary M2 material range but do not
    // receive the ground-centered primary shadow map.
    let receives_shadow = context.shadow_frame.is_some() && draw_index < context.m2_draws.len();
    let (pipeline, layout) = if receives_shadow {
        context.m2_pipelines.raw_primary_shadow(draw.pipeline())
    } else {
        context.m2_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownM2PipelineHandle)?;
    let (vertex, index) = context
        .m2_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownM2MeshHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let first_instance = u32::try_from(draw_index).map_err(|_| VulkanError::WorldFrameCapacity)?;
    let (scene_set, scene_offset) = instance_scene(context, draw.scene_index(), scene_set)?;
    let sets = [
        scene_set,
        context.frame_sets[6],
        context.frame_sets[7],
        texture,
        context.shadow_resources.m2_receiver_set(),
    ];
    bindings.draw(
        context,
        SceneCommand {
            pipeline,
            layout,
            vertex: (vertex, 0),
            index: Some((index, 0, vk::IndexType::UINT16)),
            count: draw.index_count(),
            first: draw.first_index(),
            first_instance,
            instances: instance_count,
            ..Default::default()
        }
        .parameters(
            &sets[..if receives_shadow { 5 } else { 4 }],
            &[scene_offset, 0],
            &draw.push_constants().to_bytes(),
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        ),
    )
}

/// Selects one of the three compatible stock Glue scene descriptors.
pub(super) fn m2_scene_set(
    context: &RecordContext<'_>,
    light_bank: crate::M2SceneLightBank,
) -> vk::DescriptorSet {
    context.frame_sets[3 + light_bank.index()]
}

pub(super) fn instance_scene(
    context: &RecordContext<'_>,
    index: Option<u32>,
    fixed: vk::DescriptorSet,
) -> Result<(vk::DescriptorSet, u32), VulkanError> {
    match index {
        Some(index) => Ok((
            context.frame_sets[3],
            dynamic_offset(index as usize + 8, context.m2_scene_stride)?,
        )),
        None => Ok((fixed, 0)),
    }
}

pub(in crate::device::vulkan_world_frame) fn dynamic_offset(
    index: usize,
    stride: u64,
) -> Result<u32, VulkanError> {
    (index as u64)
        .checked_mul(stride)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(VulkanError::WorldFrameCapacity)
}

/// Records one stock queue at its world/M2 stage, retaining the prepared order.
pub(super) fn record_liquid_queue(
    context: &RecordContext<'_>,
    queue: LiquidQueue,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    for (index, draw) in context.liquid_draws.iter().copied().enumerate() {
        let draw_queue = if draw.material() == LiquidDrawMaterial::Magma {
            LiquidQueue::Opaque
        } else {
            LiquidQueue::Transparent
        };
        if draw_queue != queue {
            continue;
        }
        let (pipeline, layout) = context.liquid_pipelines.raw(draw.material().shader());
        let (vertices, indices, count) =
            context.liquid_meshes.raw(draw.mesh()).ok_or_else(|| {
                VulkanError::operation("record liquid draw", "unknown liquid mesh handle")
            })?;
        let (sets, offset) = context.liquid_resources.draw_sets(index)?;
        bindings.draw(
            context,
            SceneCommand {
                pipeline,
                layout,
                vertex: (vertices, 0),
                index: Some((indices, 0, vk::IndexType::UINT16)),
                count,
                ..Default::default()
            }
            .parameters(&sets, &[offset], &[], vk::ShaderStageFlags::empty()),
        )?;
    }
    Ok(())
}
