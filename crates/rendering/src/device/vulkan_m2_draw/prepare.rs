//! Cross-registry validation before a draw can enter a frame.

use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::{M2MeshHandle, M2PipelineHandle, M2TextureSetHandle, VulkanError};
use crate::model::{M2DrawPushConstants, M2MaterialUniform, M2MeshPlan};
use crate::shader::M2MaterialState;

use super::M2PreparedDraw;

/// Joins one CPU plan draw to renderer-local resources without permitting skew.
#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    meshes: &M2MeshRegistry,
    pipelines: &M2PipelineRegistry,
    texture_sets: &M2TextureSetRegistry,
    mesh: M2MeshHandle,
    pipeline: M2PipelineHandle,
    texture_set: M2TextureSetHandle,
    plan: &M2MeshPlan,
    draw_index: usize,
    material: M2MaterialUniform,
    bone_transform_offset: u32,
    flags: u32,
) -> Result<M2PreparedDraw, VulkanError> {
    let mesh_info = meshes.info(mesh).ok_or(VulkanError::UnknownM2MeshHandle)?;
    if mesh_info.path() != plan.path() || mesh_info.profile_index() != plan.profile_index() {
        return Err(VulkanError::M2DrawMeshMismatch);
    }
    let draw = plan
        .draws()
        .get(draw_index)
        .ok_or(VulkanError::M2DrawIndex {
            requested: draw_index,
            available: plan.draws().len(),
        })?;
    let index_end = usize::try_from(draw.first_index())
        .ok()
        .and_then(|first| {
            usize::try_from(draw.index_count())
                .ok()
                .and_then(|count| first.checked_add(count))
        })
        .ok_or(VulkanError::M2DrawIndexRange)?;
    if index_end > mesh_info.index_count() {
        return Err(VulkanError::M2DrawIndexRange);
    }

    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownM2PipelineHandle)?;
    let expected_material = M2MaterialState::from_material(draw.material());
    if pipeline_info.texture_count() != draw.batch().texture_count
        || pipeline_info.material() != expected_material
    {
        return Err(VulkanError::M2DrawPipelineMismatch);
    }
    let texture_info = texture_sets
        .info(texture_set)
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    if texture_info.stage_count() != pipeline_info.texture_count() {
        return Err(VulkanError::M2DrawTextureSetMismatch);
    }

    Ok(M2PreparedDraw::new(
        mesh,
        pipeline,
        texture_set,
        draw.first_index(),
        draw.index_count(),
        material,
        M2DrawPushConstants::new(bone_transform_offset, draw, flags),
    ))
}
