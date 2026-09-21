//! Scoped presentation ownership and native servicing.

use super::VulkanRenderer;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::MeshUploadContext;
use crate::device::vulkan_texture::TextureUploadContext;

impl VulkanRenderer {
    /// Explicit offline presentation retains synchronous consumption.
    pub(super) fn with_swapchain_retry<T>(
        &mut self,
        mut present: impl FnMut(&mut Self) -> Result<T, VulkanError>,
    ) -> Result<T, VulkanError> {
        self.with_swapchain_retry_context(
            &mut (),
            |renderer, _| present(renderer),
            |_, pending| pending.wait(),
        )
    }

    /// Keeps caller execution ownership across acquisition and surface replacement.
    /// A failed present drains the existing device users before destroying handles;
    /// native callbacks can service input but cannot touch this renderer.
    pub(super) fn with_swapchain_retry_context<C, T>(
        &mut self,
        context: &mut C,
        mut present: impl FnMut(&mut Self, &mut C) -> Result<T, VulkanError>,
        mut service_native: impl FnMut(
            &mut C,
            &crate::device::GpuCompletion<'_>,
        ) -> Result<(), VulkanError>,
    ) -> Result<T, VulkanError> {
        self.collect_retired_terrain()?;
        self.collect_released_resources()?;
        if let Some(allocator) = self.allocator.as_ref() {
            self.terrain_meshes
                .retire_completed_transfers(&self.device, allocator)?;
            self.terrain_materials
                .retire_completed_transfers(&self.device, allocator)?;
            self.m2_meshes
                .retire_completed_transfers(&self.device, allocator)?;
            self.world_model_meshes
                .retire_completed_transfers(&self.device, allocator)?;
            let texture_context = TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            };
            self.ui_glyph_textures.retire_transfers(texture_context)?;
            self.character_atlas_textures
                .retire_completed_transfers(texture_context)?;
            self.blp_textures
                .retire_completed_transfers(texture_context)
                .map_err(|error| VulkanError::operation("retire BLP staging", error))?;
            self.ui_meshes.retire_transfers(MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            })?;
            self.liquid_meshes.collect(&self.device, allocator)?;
        }
        // Present may submit successfully before reporting an out-of-date surface.
        // Mark possible device use before entering it, including the first frame.
        self.is_idle = false;
        let result = match present(self, context) {
            Err(VulkanError::SwapchainOutOfDate) => {
                if let Some(completion) = self.gpu_completion.as_mut() {
                    completion.idle(|pending| service_native(context, pending))?;
                    self.is_idle = true;
                }
                self.recreate_swapchain()?;
                if let (Some(video), Some(allocator)) =
                    (self.video_capture.as_mut(), self.allocator.as_ref())
                {
                    video.resize(allocator, self.report.extent)?;
                }
                // Recreate waited for idle. A pending readback may have belonged
                // to the failed present, and must match the replacement extent.
                if self
                    .capture
                    .as_ref()
                    .is_some_and(|capture| !capture.captured)
                {
                    let allocator = self.allocator.as_ref().ok_or_else(|| {
                        VulkanError::operation(
                            "access Vulkan allocator",
                            "allocator is unavailable",
                        )
                    })?;
                    if let Some(capture) = self.capture.take() {
                        capture.destroy(allocator);
                    }
                    self.request_frame_capture()?;
                }
                self.is_idle = false;
                present(self, context)
            }
            result => result,
        };
        let screenshot_selected = self
            .capture
            .as_ref()
            .is_some_and(|capture| !capture.captured);
        if !screenshot_selected && let Some(video) = self.video_capture.as_mut() {
            video.submitted(&self.device, self.graphics_queue, result.is_ok())?;
        }
        if result.is_ok()
            && let Some(capture) = self.capture.as_mut()
        {
            capture.captured = true;
        }
        result
    }
}
