//! Startup join from retained GlueXML presentation into renderer resources.

use std::collections::HashMap;

use solarity_asset::{AssetPath, BlpTextureCache};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, UiFrameReport, UiRenderSource,
    VulkanRenderer,
};
use solarity_ui::{
    FrameManager, GlueManager, UiGlyphAtlasPlan, UiRenderPlan, UiTextureAssetBindings,
};

use crate::application::ApplicationError;
use crate::application::ui_frame::PreparedUiFrame;

/// Prepared built-in UI generation retained across FIFO-paced presentation frames.
pub(super) struct RuntimeUiFrame {
    frame: PreparedUiFrame,
}

impl RuntimeUiFrame {
    /// Uploads the current Glue generation into renderer-owned resources.
    pub(super) fn prepare_glue(
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, glue)
    }

    /// Uploads the current FrameXML generation into renderer-owned resources.
    pub(super) fn prepare_frame(
        renderer: &mut VulkanRenderer,
        frame: &FrameManager,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, frame)
    }

    /// Joins one built-in UI owner to renderer-resident mesh and textures.
    fn prepare_source(
        renderer: &mut VulkanRenderer,
        source: &impl RuntimeUiSource,
    ) -> Result<Self, ApplicationError> {
        let render_plan = source.render_plan();
        let mesh_plan = render_plan.mesh();
        let mut cache = BlpTextureCache::new();
        let bindings = source.load_blocking_render_textures(&mut cache)?;
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
                let glyphs = source.glyphs();
                renderer.upload_ui_glyph_texture(glyphs.identity(), glyphs.extent(), glyphs.rgba8())
            })
            .transpose()?;

        let glyph_texture = glyph_texture.map(|texture| (source.glyphs().identity(), texture));
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

/// Renderer-facing subset shared by retained GlueXML and FrameXML owners.
trait RuntimeUiSource {
    fn render_plan(&self) -> &UiRenderPlan;
    fn glyphs(&self) -> &UiGlyphAtlasPlan;
    fn load_blocking_render_textures(
        &self,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, solarity_ui::UiRenderError>;
}

impl RuntimeUiSource for GlueManager {
    fn render_plan(&self) -> &UiRenderPlan {
        self.render_plan()
    }

    fn glyphs(&self) -> &UiGlyphAtlasPlan {
        self.glyphs()
    }

    fn load_blocking_render_textures(
        &self,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, solarity_ui::UiRenderError> {
        self.load_blocking_render_textures(cache)
    }
}

impl RuntimeUiSource for FrameManager {
    fn render_plan(&self) -> &UiRenderPlan {
        self.render_plan()
    }

    fn glyphs(&self) -> &UiGlyphAtlasPlan {
        self.glyphs()
    }

    fn load_blocking_render_textures(
        &self,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, solarity_ui::UiRenderError> {
        self.load_blocking_render_textures(cache)
    }
}
