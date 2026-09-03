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
        cache: &mut BlpTextureCache,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, glue, cache)
    }

    /// Refreshes one live Glue generation in place when only vertex/index
    /// content changed. Texture or material topology changes still take the
    /// complete preparation path.
    pub(super) fn refresh_glue(
        &mut self,
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
        cache: &mut BlpTextureCache,
    ) -> Result<(), ApplicationError> {
        if self
            .frame
            .try_replace_compatible_mesh(renderer, glue.render_plan().mesh())?
        {
            return Ok(());
        }
        *self = Self::prepare_source(renderer, glue, cache)?;
        Ok(())
    }

    /// Uploads the current FrameXML generation into renderer-owned resources.
    pub(super) fn prepare_frame(
        renderer: &mut VulkanRenderer,
        frame: &FrameManager,
        cache: &mut BlpTextureCache,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, frame, cache)
    }

    /// Joins one built-in UI owner to renderer-resident mesh and textures.
    fn prepare_source(
        renderer: &mut VulkanRenderer,
        source: &impl RuntimeUiSource,
        cache: &mut BlpTextureCache,
    ) -> Result<Self, ApplicationError> {
        let started = std::time::Instant::now();
        let render_plan = source.render_plan();
        let mesh_plan = render_plan.mesh();
        let asset_started = std::time::Instant::now();
        let bindings = source.load_blocking_render_textures(cache)?;
        let asset_elapsed = asset_started.elapsed();
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
        let texture_count = texture_uploads.len();
        let upload_started = std::time::Instant::now();
        let uploaded = renderer.upload_blp_textures(&texture_uploads)?;
        let upload_elapsed = upload_started.elapsed();
        for (path, handle) in texture_paths.into_iter().zip(uploaded) {
            textures.insert(path, handle);
        }
        let glyph_started = std::time::Instant::now();
        let glyph_texture = mesh_plan
            .batches()
            .iter()
            .any(|batch| matches!(batch.source(), UiRenderSource::GlyphAtlas(_)))
            .then(|| {
                let glyphs = source.glyphs();
                renderer.upload_ui_glyph_texture(glyphs.identity(), glyphs.extent(), glyphs.rgba8())
            })
            .transpose()?;
        let glyph_elapsed = glyph_started.elapsed();

        let glyph_texture = glyph_texture.map(|texture| (source.glyphs().identity(), texture));
        let frame_started = std::time::Instant::now();
        let frame = PreparedUiFrame::prepare(renderer, mesh_plan, &textures, glyph_texture)?;
        let frame_elapsed = frame_started.elapsed();
        tracing::info!(
            texture_count,
            batch_count = mesh_plan.batches().len(),
            vertex_count = mesh_plan.vertices().len(),
            asset_ms = asset_elapsed.as_secs_f64() * 1_000.0,
            texture_upload_ms = upload_elapsed.as_secs_f64() * 1_000.0,
            glyph_upload_ms = glyph_elapsed.as_secs_f64() * 1_000.0,
            frame_prepare_ms = frame_elapsed.as_secs_f64() * 1_000.0,
            total_ms = started.elapsed().as_secs_f64() * 1_000.0,
            "prepared built-in UI generation"
        );
        Ok(Self { frame })
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
