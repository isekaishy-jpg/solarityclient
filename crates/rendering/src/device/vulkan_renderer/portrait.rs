//! Public portrait rendering boundary and retained archive-mask draw.

use super::*;
use crate::device::vulkan_m2_frame::{PortraitRenderContext, UiPortraitTextureHandle};
use crate::{UiRenderQuad, UiRenderSource, UiTextureAddressMode, UiTextureResidency};

impl VulkanRenderer {
    /// Returns the last submitted portrait image for this exact unit token.
    #[must_use]
    pub fn unit_portrait_texture(&self, unit: &str) -> Option<UiPortraitTextureHandle> {
        self.portraits.handle(unit)
    }

    /// Renders a frozen model snapshot to a reusable 64 by 64 sampled image.
    /// The supplied archive mask replaces alpha after model rendering. The
    /// caller determines camera, lighting, equipment, and appearance invalidation.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for invalid resources, insufficient bones,
    /// unsupported shadow draws, allocation, recording, or submission failures.
    pub fn render_unit_portrait(
        &mut self,
        unit: &str,
        scene: M2SceneUniform,
        bone_transforms: &[Mat4],
        draws: &[M2PreparedDraw],
        alpha_mask: BlpTextureHandle,
    ) -> Result<UiPortraitTextureHandle, VulkanError> {
        let mask = self.prepare_portrait_mask(alpha_mask)?;
        let frame_layouts = self.m2_pipelines.frame_set_layouts(&self.device)?;
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access portrait allocator", "allocator is unavailable")
        })?;
        let result = self.portraits.render(
            PortraitRenderContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                color_format: self.color_format,
                depth_format: self.depth_format,
                uniform_alignment: self.uniform_buffer_alignment,
                storage_alignment: self.storage_buffer_alignment,
                frame_layouts,
                pipelines: &self.m2_pipelines,
                meshes: &self.m2_meshes,
                texture_sets: &self.m2_texture_sets,
                ui_pipelines: &self.ui_pipelines,
                ui_meshes: &self.ui_meshes,
                ui_texture_sets: &self.ui_texture_sets,
                mask,
            },
            unit,
            scene,
            bone_transforms,
            draws,
        )?;
        self.is_idle = false;
        Ok(result)
    }

    fn prepare_portrait_mask(
        &mut self,
        texture: BlpTextureHandle,
    ) -> Result<UiPreparedDraw, VulkanError> {
        if let Some(draw) = self.portrait_masks.get(&texture) {
            return Ok(*draw);
        }
        let path = self
            .blp_textures
            .info(texture)
            .ok_or(VulkanError::UnknownBlpTextureHandle)?
            .path()
            .clone();
        let quad = UiRenderQuad::new(
            0,
            UiRenderSource::Texture(path),
            UiRenderBlend::AlphaMask,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            UiTextureResidency::Blocking,
            false,
            [0.0, 0.0, 64.0, 64.0],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
            [[1.0; 4]; 4],
        );
        let plan = UiMeshPlan::prepare([64.0, 64.0], [quad].into_iter())
            .map_err(|source| VulkanError::operation("prepare portrait mask quad", source))?;
        let mesh = self.upload_ui_mesh(&plan)?;
        let pipeline =
            self.prepare_ui_pipeline(UiShaderSource::Texture, UiRenderBlend::AlphaMask)?;
        let sampler = self.prepare_ui_sampler(UiSamplerInfo::new(
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
        ))?;
        let sets = self.prepare_ui_texture_sets(&[UiSampledTexture::new(texture, sampler)])?;
        let draw = self.prepare_ui_draw(mesh, pipeline, Some(sets[0]), &plan, 0)?;
        self.portrait_masks.insert(texture, draw);
        Ok(draw)
    }
}
