//! Startup join from retained GlueXML presentation into renderer resources.

use std::collections::HashMap;

use solarity_asset::{AssetPath, BlpTextureCache};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, UiFrameReport, UiRenderSource,
    VulkanRenderer,
};
use solarity_ui::GlueManager;

use crate::application::ApplicationError;
use crate::application::ui_frame::PreparedUiFrame;

/// Prepared login generation retained across FIFO-paced presentation frames.
pub(super) struct LoginUiFrame {
    frame: PreparedUiFrame,
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
        let mut texture_paths = Vec::new();
        let mut texture_uploads = Vec::new();
        for (request_index, request) in render_plan.texture_assets().requests().iter().enumerate() {
            let Some(source) = bindings.source(request_index) else {
                // Stock marks this source non-blocking. Its material batch remains
                // absent until the streaming owner publishes a resident image.
                continue;
            };
            // Build 12340's fixed-function UI path samples color bytes linearly;
            // the BLP container itself carries no transfer-function metadata.
            texture_paths.push(request.path().clone());
            texture_uploads.push(BlpTextureUploadRequest::new(source, BlpColorSpace::Linear));
        }
        for (path, handle) in texture_paths
            .into_iter()
            .zip(renderer.upload_blp_textures(&texture_uploads)?)
        {
            textures.insert(path, handle);
        }
        let glyph_texture = mesh_plan
            .batches()
            .iter()
            .any(|batch| matches!(batch.source(), UiRenderSource::GlyphAtlas(_)))
            .then(|| {
                let glyphs = glue.glyphs();
                renderer.upload_ui_glyph_texture(glyphs.identity(), glyphs.extent(), glyphs.rgba8())
            })
            .transpose()?;

        let glyph_texture = glyph_texture.map(|texture| (glue.glyphs().identity(), texture));
        Ok(Self {
            frame: PreparedUiFrame::prepare(renderer, mesh_plan, &textures, glyph_texture)?,
        })
    }

    /// Queues this Glue generation followed by an independent overlay.
    pub(super) fn present_with_overlay(
        &self,
        renderer: &mut VulkanRenderer,
        overlay: &[solarity_rendering::UiPreparedDraw],
    ) -> Result<UiFrameReport, ApplicationError> {
        self.frame.present_with_overlay(renderer, overlay)
    }

    /// Returns the logical UI canvas used by the overlay shader.
    pub(super) const fn logical_extent(&self) -> [f32; 2] {
        self.frame.logical_extent()
    }

    /// Returns renderer-validated UI packets for composite presentation.
    pub(super) fn draws(&self) -> &[solarity_rendering::UiPreparedDraw] {
        self.frame.draws()
    }
}
