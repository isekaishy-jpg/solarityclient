//! Renderer-resident UI mesh, material, and sampled-image composition.

use std::collections::HashMap;

use solarity_asset::AssetPath;
use solarity_rendering::{
    BlpTextureHandle, UiFrameReport, UiGlyphTextureHandle, UiMeshPlan, UiPreparedDraw,
    UiRenderSource, UiSampledTexture, UiSamplerInfo, UiShaderSource, UiTextureResidency,
    VulkanError, VulkanRenderer,
};

use crate::application::ApplicationError;

/// One immutable UI mesh generation joined to renderer-owned resources.
pub(crate) struct PreparedUiFrame {
    logical_extent: [f32; 2],
    draws: Vec<UiPreparedDraw>,
}

impl PreparedUiFrame {
    /// Uploads one complete mesh and prepares its ordered material packets.
    pub(crate) fn prepare(
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
        textures: &HashMap<AssetPath, BlpTextureHandle>,
        glyph_texture: Option<(u64, UiGlyphTextureHandle)>,
    ) -> Result<Self, ApplicationError> {
        let mesh = renderer.upload_ui_mesh(plan)?;
        let mut batch_resources = Vec::with_capacity(plan.batches().len());
        let mut sampled_textures = Vec::new();
        for (batch_index, batch) in plan.batches().iter().enumerate() {
            let source = match batch.source() {
                UiRenderSource::Texture(_) | UiRenderSource::GlyphAtlas(_) => {
                    UiShaderSource::Texture
                }
                UiRenderSource::VertexColor => UiShaderSource::VertexColor,
            };
            let pipeline = renderer.prepare_ui_pipeline(source, batch.blend())?;
            let sampled_index = match batch.source() {
                UiRenderSource::Texture(path) => {
                    let Some(texture) = textures.get(path).copied() else {
                        if batch.residency() == UiTextureResidency::NonBlocking {
                            continue;
                        }
                        return Err(VulkanError::UiDrawTextureMismatch.into());
                    };
                    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
                        batch.horizontal_address(),
                        batch.vertical_address(),
                    ))?;
                    let index = sampled_textures.len();
                    sampled_textures.push(UiSampledTexture::new(texture, sampler));
                    Some(index)
                }
                UiRenderSource::GlyphAtlas(identity) => {
                    let (expected_identity, texture) =
                        glyph_texture.ok_or(VulkanError::UnknownUiGlyphTextureHandle)?;
                    if *identity != expected_identity {
                        return Err(VulkanError::UiDrawTextureMismatch.into());
                    }
                    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
                        batch.horizontal_address(),
                        batch.vertical_address(),
                    ))?;
                    let index = sampled_textures.len();
                    sampled_textures.push(UiSampledTexture::glyph(texture, sampler));
                    Some(index)
                }
                UiRenderSource::VertexColor => None,
            };
            batch_resources.push((batch_index, pipeline, sampled_index));
        }
        let texture_sets = renderer.prepare_ui_texture_sets(&sampled_textures)?;
        let draws = batch_resources
            .into_iter()
            .map(|(batch_index, pipeline, sampled_index)| {
                renderer.prepare_ui_draw(
                    mesh,
                    pipeline,
                    sampled_index.map(|index| texture_sets[index]),
                    plan,
                    batch_index,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            logical_extent: plan.logical_extent(),
            draws,
        })
    }

    /// Presents this generation followed by one independently retained overlay.
    pub(crate) fn present_with_overlay(
        &self,
        renderer: &mut VulkanRenderer,
        overlay: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, ApplicationError> {
        let mut draws = Vec::with_capacity(self.draws.len() + overlay.len());
        draws.extend_from_slice(&self.draws);
        draws.extend_from_slice(overlay);
        renderer
            .present_ui(self.logical_extent, &draws)
            .map_err(ApplicationError::from)
    }

    /// Replaces this frame's mesh while retaining compatible material resources.
    pub(crate) fn replace_mesh(
        &mut self,
        renderer: &mut VulkanRenderer,
        plan: &UiMeshPlan,
    ) -> Result<(), ApplicationError> {
        if plan.batches().len() != self.draws.len() {
            return Err(VulkanError::UiDrawIndex {
                requested: plan.batches().len(),
                available: self.draws.len(),
            }
            .into());
        }
        let mesh = self
            .draws
            .first()
            .map(|draw| draw.mesh())
            .ok_or(VulkanError::EmptyUiFrame)?;
        renderer.replace_ui_mesh(mesh, plan)?;
        self.draws = self
            .draws
            .iter()
            .enumerate()
            .map(|(batch_index, draw)| {
                renderer.prepare_ui_draw(
                    mesh,
                    draw.pipeline(),
                    draw.texture_set(),
                    plan,
                    batch_index,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.logical_extent = plan.logical_extent();
        Ok(())
    }

    pub(crate) const fn logical_extent(&self) -> [f32; 2] {
        self.logical_extent
    }

    pub(crate) fn draws(&self) -> &[UiPreparedDraw] {
        &self.draws
    }
}
