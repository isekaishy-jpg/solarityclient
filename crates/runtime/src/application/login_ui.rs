//! Startup join from retained GlueXML presentation into renderer resources.

use std::collections::HashMap;

use solarity_asset::{AssetPath, BlpTextureCache};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, UiFrameReport, UiPreparedDraw, UiRenderSource,
    UiSampledTexture, UiSamplerInfo, UiShaderSource, VulkanRenderer,
};
use solarity_ui::GlueManager;

use crate::application::ApplicationError;

/// Prepared login generation retained across FIFO-paced presentation frames.
pub(super) struct LoginUiFrame {
    logical_extent: [f32; 2],
    draws: Vec<UiPreparedDraw>,
}

impl LoginUiFrame {
    /// Uploads the current Glue generation into renderer-owned resources.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
    ) -> Result<Self, ApplicationError> {
        let render_plan = glue.render_plan();
        let mesh_plan = render_plan.mesh();
        let mut cache = BlpTextureCache::new();
        let bindings = glue.load_blocking_render_textures(&mut cache)?;
        let mut textures = HashMap::<AssetPath, BlpTextureHandle>::new();
        for (request_index, request) in render_plan.texture_assets().requests().iter().enumerate() {
            let Some(source) = bindings.source(request_index) else {
                // Stock marks this source non-blocking. Its material batch remains
                // absent until the streaming owner publishes a resident image.
                continue;
            };
            // Build 12340's fixed-function UI path samples color bytes linearly;
            // the BLP container itself carries no transfer-function metadata.
            let handle = renderer.upload_blp_texture(source, BlpColorSpace::Linear)?;
            textures.insert(request.path().clone(), handle);
        }

        let mesh = renderer.upload_ui_mesh(mesh_plan)?;
        let mut batch_resources = Vec::with_capacity(mesh_plan.batches().len());
        let mut sampled_textures = Vec::new();
        for (batch_index, batch) in mesh_plan.batches().iter().enumerate() {
            let source = match batch.source() {
                UiRenderSource::Texture(_) => UiShaderSource::Texture,
                UiRenderSource::VertexColor => UiShaderSource::VertexColor,
            };
            let pipeline = renderer.prepare_ui_pipeline(source, batch.blend())?;
            let sampled_index = match batch.source() {
                UiRenderSource::Texture(path) => {
                    let Some(texture) = textures.get(path).copied() else {
                        continue;
                    };
                    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
                        batch.horizontal_address(),
                        batch.vertical_address(),
                    ))?;
                    let index = sampled_textures.len();
                    sampled_textures.push(UiSampledTexture::new(texture, sampler));
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
                let texture_set = sampled_index.map(|index| texture_sets[index]);
                renderer.prepare_ui_draw(mesh, pipeline, texture_set, mesh_plan, batch_index)
            })
            .collect::<Result<Vec<UiPreparedDraw>, _>>()?;
        Ok(Self {
            logical_extent: mesh_plan.logical_extent(),
            draws,
        })
    }

    /// Queues this immutable generation as the next swapchain frame.
    pub(super) fn present(
        &self,
        renderer: &mut VulkanRenderer,
    ) -> Result<UiFrameReport, ApplicationError> {
        renderer
            .present_ui(self.logical_extent, &self.draws)
            .map_err(ApplicationError::from)
    }
}
