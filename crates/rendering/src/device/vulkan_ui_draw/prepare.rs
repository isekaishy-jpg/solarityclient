//! Validation preventing CPU plans and renderer-local resources from skewing.

use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::vulkan_ui_glyph_texture::UiGlyphTextureRegistry;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_sampler::UiSamplerRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;
use crate::device::{UiMeshHandle, UiPipelineHandle, UiTextureSetHandle, VulkanError};
use crate::{UiMeshPlan, UiRenderSource, UiSamplerInfo, UiShaderSource, UiTextureImageHandle};

use super::UiPreparedDraw;

/// Joins one ordered CPU batch to compatible renderer-local resources.
#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    meshes: &UiMeshRegistry,
    pipelines: &UiPipelineRegistry,
    texture_sets: &UiTextureSetRegistry,
    textures: &BlpTextureRegistry,
    glyphs: &UiGlyphTextureRegistry,
    samplers: &UiSamplerRegistry,
    mesh: UiMeshHandle,
    pipeline: UiPipelineHandle,
    texture_set: Option<UiTextureSetHandle>,
    plan: &UiMeshPlan,
    batch_index: usize,
) -> Result<UiPreparedDraw, VulkanError> {
    let mesh_info = meshes.info(mesh).ok_or(VulkanError::UnknownUiMeshHandle)?;
    if mesh_info.plan_identity() != plan.identity() {
        return Err(VulkanError::UiDrawMeshMismatch);
    }
    let batch = plan
        .batches()
        .get(batch_index)
        .ok_or(VulkanError::UiDrawIndex {
            requested: batch_index,
            available: plan.batches().len(),
        })?;
    let index_end = usize::try_from(batch.first_index())
        .ok()
        .and_then(|first| {
            usize::try_from(batch.index_count())
                .ok()
                .and_then(|count| first.checked_add(count))
        })
        .ok_or(VulkanError::UiDrawIndexRange)?;
    if index_end > mesh_info.index_count() {
        return Err(VulkanError::UiDrawIndexRange);
    }
    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownUiPipelineHandle)?;
    let expected_source = match batch.source() {
        UiRenderSource::Texture(_) | UiRenderSource::GlyphAtlas(_) => UiShaderSource::Texture,
        UiRenderSource::VertexColor => UiShaderSource::VertexColor,
    };
    if pipeline_info.source() != expected_source || pipeline_info.blend() != batch.blend() {
        return Err(VulkanError::UiDrawPipelineMismatch);
    }
    match (batch.source(), texture_set) {
        (UiRenderSource::VertexColor, None) => {}
        (UiRenderSource::Texture(path), Some(handle)) => {
            let set = texture_sets
                .info(handle)
                .ok_or(VulkanError::UnknownUiTextureSetHandle)?;
            let sampled = set.sampled_texture();
            let UiTextureImageHandle::Blp(texture_handle) = sampled.texture() else {
                return Err(VulkanError::UiDrawTextureMismatch);
            };
            let texture = textures
                .info(texture_handle)
                .ok_or(VulkanError::UnknownBlpTextureHandle)?;
            let sampler = samplers
                .info(sampled.sampler())
                .ok_or(VulkanError::UnknownUiSamplerHandle)?;
            let expected_sampler =
                UiSamplerInfo::new(batch.horizontal_address(), batch.vertical_address());
            if texture.path() != path || sampler != expected_sampler {
                return Err(VulkanError::UiDrawTextureMismatch);
            }
        }
        (UiRenderSource::GlyphAtlas(identity), Some(handle)) => {
            let set = texture_sets
                .info(handle)
                .ok_or(VulkanError::UnknownUiTextureSetHandle)?;
            let sampled = set.sampled_texture();
            let UiTextureImageHandle::Glyph(texture_handle) = sampled.texture() else {
                return Err(VulkanError::UiDrawTextureMismatch);
            };
            let texture = glyphs
                .info(texture_handle)
                .ok_or(VulkanError::UnknownUiGlyphTextureHandle)?;
            let sampler = samplers
                .info(sampled.sampler())
                .ok_or(VulkanError::UnknownUiSamplerHandle)?;
            let expected_sampler =
                UiSamplerInfo::new(batch.horizontal_address(), batch.vertical_address());
            if texture.identity() != *identity || sampler != expected_sampler {
                return Err(VulkanError::UiDrawTextureMismatch);
            }
        }
        _ => return Err(VulkanError::UiDrawTextureMismatch),
    }
    let base_vertex = i32::try_from(u64::from(batch.first_quad()) * 4)
        .map_err(|_source| VulkanError::UiDrawIndexRange)?;
    Ok(UiPreparedDraw::new(
        mesh,
        pipeline,
        texture_set,
        batch.first_index(),
        batch.index_count(),
        base_vertex,
        batch.translation(),
        batch.clip(),
    ))
}
