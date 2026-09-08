//! Cross-registry validation before an MCNK can enter a frame.

use crate::device::vulkan_terrain_material::TerrainMaterialRegistry;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::{
    BlpColorSpace, TerrainMeshHandle, TerrainPipelineHandle, TerrainTextureSet,
    TerrainTextureSetHandle, VulkanError,
};
use crate::{TerrainLayerCount, TerrainTileMeshPlan};

use super::TerrainPreparedDraw;

#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    meshes: &TerrainMeshRegistry,
    materials: &TerrainMaterialRegistry,
    pipelines: &TerrainPipelineRegistry,
    texture_sets: &TerrainTextureSetRegistry,
    textures: &BlpTextureRegistry,
    mesh: TerrainMeshHandle,
    pipeline: TerrainPipelineHandle,
    texture_set: TerrainTextureSetHandle,
    texture_request: &TerrainTextureSet,
    plan: &TerrainTileMeshPlan,
    chunk_index: usize,
) -> Result<TerrainPreparedDraw, VulkanError> {
    let mesh_info = meshes
        .info(mesh)
        .ok_or(VulkanError::UnknownTerrainMeshHandle)?;
    if !meshes.matches_plan(mesh, plan) {
        return Err(VulkanError::TerrainDrawMeshMismatch);
    }
    let chunk = plan
        .chunks()
        .get(chunk_index)
        .ok_or(VulkanError::TerrainDrawIndex {
            requested: chunk_index,
            available: plan.chunks().len(),
        })?;
    let index_end = usize::try_from(chunk.first_index())
        .ok()
        .and_then(|first| {
            usize::try_from(chunk.index_count())
                .ok()
                .and_then(|count| first.checked_add(count))
        })
        .ok_or(VulkanError::TerrainDrawIndexRange)?;
    if index_end > mesh_info.index_count() {
        return Err(VulkanError::TerrainDrawIndexRange);
    }
    let layer_count = TerrainLayerCount::try_from(chunk.layers().len())
        .map_err(|_source| VulkanError::TerrainDrawPipelineMismatch)?;
    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownTerrainPipelineHandle)?;
    if pipeline_info.layer_count() != layer_count {
        return Err(VulkanError::TerrainDrawPipelineMismatch);
    }
    if !texture_sets.matches(texture_set, texture_request) {
        return Err(VulkanError::UnknownTerrainTextureSetHandle);
    }
    if texture_request.layer_count() != layer_count
        || !materials.matches_plan(texture_request.material(), plan)
    {
        return Err(VulkanError::TerrainDrawTextureSetMismatch);
    }
    for (layer, handle) in chunk.layers().iter().zip(texture_request.layers()) {
        let texture_index = usize::try_from(layer.texture_index())
            .map_err(|_source| VulkanError::TerrainDrawTextureSetMismatch)?;
        let expected_path = plan
            .textures()
            .get(texture_index)
            .ok_or(VulkanError::TerrainDrawTextureSetMismatch)?;
        let actual = textures
            .info(*handle)
            .ok_or(VulkanError::TerrainDrawTextureSetMismatch)?;
        if actual.path() != expected_path || actual.color_space() != BlpColorSpace::Linear {
            return Err(VulkanError::TerrainDrawTextureSetMismatch);
        }
    }
    Ok(TerrainPreparedDraw::new(
        mesh,
        pipeline,
        texture_set,
        chunk.first_index(),
        chunk.index_count(),
        chunk.atlas_chunk(),
    ))
}
