//! Startup join from retained GlueXML presentation into renderer resources.

mod sources;

use std::collections::HashMap;

use solarity_asset::{AssetPath, BlpTextureCache};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, UiFrameReport, UiGlyphTextureHandle,
    VulkanRenderer,
};
use solarity_ui::{FrameManager, GlueManager, UiGlyphAtlasPlan, UiRenderPlan};

use crate::application::ApplicationError;
use crate::application::ui_frame::PreparedUiFrame;

/// Prepared built-in UI generation retained across FIFO-paced presentation frames.
pub(super) struct RuntimeUiFrame {
    frame: PreparedUiFrame,
    coverage: (u64, u64),
}

/// Sampled images retained across every pre-world UI mesh generation.
pub(super) struct RuntimeUiResidency {
    sources: sources::UiTextureLoader,
    textures: HashMap<AssetPath, BlpTextureHandle>,
    glyph_textures: HashMap<u64, UiGlyphTextureHandle>,
    glyph_revisions: HashMap<u64, u64>,
    glyph_owners: HashMap<u64, (std::sync::Weak<()>, solarity_rendering::GpuResourceLease)>,
}

impl RuntimeUiResidency {
    pub(super) fn new(catalog: solarity_asset::ArchiveCatalog) -> Self {
        Self {
            sources: sources::UiTextureLoader::new(catalog),
            textures: HashMap::new(),
            glyph_textures: HashMap::new(),
            glyph_revisions: HashMap::new(),
            glyph_owners: HashMap::new(),
        }
    }

    /// Synchronizes only coverage added since the last page publication.
    fn synchronize_glyphs(
        &mut self,
        renderer: &mut VulkanRenderer,
        atlas: &UiGlyphAtlasPlan,
    ) -> Result<(), ApplicationError> {
        self.glyph_owners.retain(|identity, (owner, _lease)| {
            if owner.strong_count() != 0 {
                return true;
            }
            self.glyph_textures.remove(identity);
            self.glyph_revisions.remove(identity);
            false
        });
        for page in atlas.pages() {
            if let Some(&handle) = self.glyph_textures.get(&page.identity()) {
                let revision = self
                    .glyph_revisions
                    .get(&page.identity())
                    .copied()
                    .unwrap_or(0);
                if revision == page.revision() {
                    continue;
                }
                let changes = page.changes_since(revision).collect::<Vec<_>>();
                renderer.update_ui_glyph_texture(handle, page.extent(), page.rgba8(), &changes)?;
            } else {
                let handle = renderer.upload_ui_glyph_texture(
                    page.identity(),
                    page.extent(),
                    page.rgba8(),
                )?;
                self.glyph_textures.insert(page.identity(), handle);
                self.glyph_owners.insert(
                    page.identity(),
                    (
                        std::sync::Arc::downgrade(&page.lifetime()),
                        renderer.retain_ui_glyph_texture(handle)?,
                    ),
                );
            }
            self.glyph_revisions
                .insert(page.identity(), page.revision());
        }
        Ok(())
    }

    /// Uploads decoded UI sources that are not already renderer-resident.
    pub(super) fn prewarm(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        cache: &BlpTextureCache,
        namespace: solarity_asset::AssetNamespaceId,
    ) -> Result<usize, ApplicationError> {
        let mut paths = Vec::new();
        let mut uploads = Vec::new();
        for (path, source) in cache.entries(namespace) {
            if self.textures.contains_key(path) {
                continue;
            }
            paths.push(path.clone());
            uploads.push(BlpTextureUploadRequest::new(source, BlpColorSpace::Linear));
        }
        let upload_count = uploads.len();
        if upload_count == 0 {
            return Ok(0);
        }
        for (path, handle) in paths
            .into_iter()
            .zip(renderer.upload_blp_textures(&uploads)?)
        {
            self.textures.insert(path, handle);
        }
        Ok(upload_count)
    }
}

impl RuntimeUiFrame {
    /// Uploads the current Glue generation into renderer-owned resources.
    pub(super) fn prepare_glue(
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        glue: &GlueManager,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, glue, cache, residency)
    }

    /// Refreshes one live Glue generation in place. Retained scroll state only
    /// changes draw push constants/scissors; geometry or material changes use
    /// the compatible replacement or complete preparation paths.
    pub(super) fn refresh_glue(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        glue: &GlueManager,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<(), ApplicationError> {
        self.refresh_source(renderer, glue, cache, residency)
    }

    /// Refreshes one live FrameXML generation through the same retained mesh
    /// and material path as GlueXML.
    pub(super) fn refresh_frame(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        frame: &FrameManager,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<(), ApplicationError> {
        self.refresh_source(renderer, frame, cache, residency)
    }

    fn refresh_source(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        source: &impl RuntimeUiSource,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<(), ApplicationError> {
        let coverage = (
            source.glyphs().identity(),
            source.glyphs().coverage_revision(),
        );
        if self.coverage != coverage {
            residency.synchronize_glyphs(renderer, source.glyphs())?;
            self.coverage = coverage;
        }
        if self
            .frame
            .try_replace_compatible_mesh(renderer, source.render_plan().mesh())?
        {
            return Ok(());
        }
        let frame = Self::prepare_source_with_resources(
            renderer,
            source,
            cache,
            Some(self.frame.mesh()),
            residency,
        )?;
        self.frame = frame;
        Ok(())
    }

    /// Uploads the current FrameXML generation into renderer-owned resources.
    pub(super) fn prepare_frame(
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        frame: &FrameManager,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<Self, ApplicationError> {
        Self::prepare_source(renderer, frame, cache, residency)
    }

    /// Joins one built-in UI owner to renderer-resident mesh and textures.
    fn prepare_source(
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        source: &impl RuntimeUiSource,
        cache: &mut BlpTextureCache,
        residency: &mut RuntimeUiResidency,
    ) -> Result<Self, ApplicationError> {
        residency.synchronize_glyphs(renderer, source.glyphs())?;
        let frame = Self::prepare_source_with_resources(renderer, source, cache, None, residency)?;
        Ok(Self {
            frame,
            coverage: (
                source.glyphs().identity(),
                source.glyphs().coverage_revision(),
            ),
        })
    }

    /// Prepares material resources while retaining process-long sampled images.
    fn prepare_source_with_resources(
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        source: &impl RuntimeUiSource,
        cache: &mut BlpTextureCache,
        retained_mesh: Option<solarity_rendering::UiMeshHandle>,
        residency: &mut RuntimeUiResidency,
    ) -> Result<PreparedUiFrame, ApplicationError> {
        let started = std::time::Instant::now();
        let render_plan = source.render_plan();
        let mesh_plan = render_plan.mesh();
        let asset_started = std::time::Instant::now();
        let texture_paths = sources::missing_paths(render_plan.texture_assets(), |path| {
            residency.textures.contains_key(path)
        });
        let sources = residency.sources.load(renderer, cache, texture_paths)?;
        let asset_elapsed = asset_started.elapsed();
        // Build 12340's fixed-function UI path samples color bytes linearly;
        // the BLP container itself carries no transfer-function metadata.
        let texture_uploads = sources
            .iter()
            .map(|source| BlpTextureUploadRequest::new(source, BlpColorSpace::Linear))
            .collect::<Vec<_>>();
        let texture_count = texture_uploads.len();
        let upload_started = std::time::Instant::now();
        let uploaded = if texture_uploads.is_empty() {
            Vec::new()
        } else {
            renderer.upload_blp_textures(&texture_uploads)?
        };
        let upload_elapsed = upload_started.elapsed();
        for (source, handle) in sources.into_iter().zip(uploaded) {
            residency.textures.insert(source.path().clone(), handle);
        }
        let glyph_started = std::time::Instant::now();
        let glyph_elapsed = glyph_started.elapsed();
        let frame_started = std::time::Instant::now();
        let frame = PreparedUiFrame::prepare_pages(
            renderer,
            mesh_plan,
            &residency.textures,
            &residency.glyph_textures,
            retained_mesh,
        )?;
        let frame_elapsed = frame_started.elapsed();
        // Animated GlueXML can replace this mesh every presentation frame.
        // Keep the phase timings available for an explicit debug subscriber;
        // formatting and writing them at INFO otherwise introduces visible
        // main-thread frame-time spikes in the ordinary file-log profile.
        tracing::debug!(
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
        Ok(frame)
    }

    /// Queues this Glue generation followed by an independent overlay.
    pub(super) fn present_with_overlay(
        &self,
        renderer: &mut VulkanRenderer,
        overlay: &[solarity_rendering::UiPreparedDraw],
        service_native: &mut impl FnMut(
            &solarity_rendering::GpuCompletion<'_>,
        ) -> Result<(), solarity_rendering::VulkanError>,
    ) -> Result<UiFrameReport, ApplicationError> {
        self.frame
            .present_with_overlay(renderer, overlay, service_native)
    }

    /// Returns the logical UI canvas used by the overlay shader.
    pub(super) const fn logical_extent(&self) -> [f32; 2] {
        self.frame.logical_extent()
    }

    /// Returns renderer-validated UI packets for composite presentation.
    pub(super) fn draws(&self) -> &[solarity_rendering::UiPreparedDraw] {
        self.frame.draws()
    }

    pub(super) fn draw_insertion_index(&self, batch_index: usize) -> usize {
        self.frame.draw_insertion_index(batch_index)
    }
}

/// Renderer-facing subset shared by retained GlueXML and FrameXML owners.
trait RuntimeUiSource {
    fn render_plan(&self) -> &UiRenderPlan;
    fn glyphs(&self) -> &UiGlyphAtlasPlan;
}

impl RuntimeUiSource for GlueManager {
    fn render_plan(&self) -> &UiRenderPlan {
        self.render_plan()
    }

    fn glyphs(&self) -> &UiGlyphAtlasPlan {
        self.glyphs()
    }
}

impl RuntimeUiSource for FrameManager {
    fn render_plan(&self) -> &UiRenderPlan {
        self.render_plan()
    }

    fn glyphs(&self) -> &UiGlyphAtlasPlan {
        self.glyphs()
    }
}
