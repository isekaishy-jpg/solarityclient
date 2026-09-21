//! Scoped presentation ownership and native servicing.

use super::VulkanRenderer;
use crate::device::VulkanError;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_frame::{UiFrameContext, UiFrameReport};

impl VulkanRenderer {
    /// Records and presents one complete ordered UI batch list.
    ///
    /// Logical extent is the coordinate space used when the immutable mesh was
    /// prepared. It is independent from the physical swapchain extent so stock
    /// UI geometry scales without rebuilding or special-casing HD textures.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for an empty frame, invalid logical extent,
    /// swapchain resource mismatch, command recording, submission, or present
    /// failure.
    pub fn present_ui(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| renderer.present_ui_once(logical_extent, draws))
    }

    /// Presents one UI generation followed by an independently retained overlay.
    ///
    /// Both slices remain borrowed through command recording, avoiding a
    /// per-frame concatenation allocation while preserving their exact order.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] under the same conditions as [`Self::present_ui`].
    pub fn present_ui_with_overlay(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
        overlay: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        self.present_ui_with_overlay_serviced(logical_extent, draws, overlay, &mut |pending| {
            pending.wait()
        })
    }

    /// Presents retained UI while servicing native input during pending acquisition.
    /// The callback cannot mutate this exclusively borrowed renderer.
    ///
    /// # Errors
    /// Returns validation, presentation, or native servicing failures.
    pub fn present_ui_with_overlay_serviced(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
        overlay: &[UiPreparedDraw],
        service_native: &mut impl FnMut(&crate::device::GpuCompletion<'_>) -> Result<(), VulkanError>,
    ) -> Result<UiFrameReport, VulkanError> {
        self.with_swapchain_retry_context(
            service_native,
            |renderer, service_native| {
                renderer.present_ui_with_overlay_once(
                    logical_extent,
                    draws,
                    overlay,
                    service_native,
                )
            },
            |service_native, pending| service_native(pending),
        )
    }

    /// Clears the current swapchain image to opaque black and presents it.
    ///
    /// This is the explicit handoff surface used between independently loaded
    /// presentation domains, where retaining the previous frame would expose
    /// stale cinematic or loading-screen contents.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for an invalid logical extent, swapchain
    /// acquisition, command recording, submission, or presentation failure.
    pub fn present_clear(&mut self, logical_extent: [f32; 2]) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| renderer.present_clear_once(logical_extent))
    }

    fn present_clear_once(&mut self, logical_extent: [f32; 2]) -> Result<(), VulkanError> {
        let report = self.ui_frames.present_clear(
            UiFrameContext {
                gpu_completion: self.gpu_completion.as_mut(),
                device: &self.device,
                capture: self
                    .capture
                    .as_ref()
                    .filter(|capture| !capture.captured)
                    .or_else(|| {
                        self.video_capture
                            .as_ref()
                            .and_then(|video| video.pending())
                    }),
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                pipelines: &self.ui_pipelines,
                meshes: &self.ui_meshes,
                texture_sets: &self.ui_texture_sets,
            },
            logical_extent,
        )?;
        debug_assert_eq!(report.draw_count(), 0);
        self.is_idle = false;
        Ok(())
    }

    fn present_ui_once(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        self.present_ui_with_overlay_once(logical_extent, draws, &[], &mut |pending| pending.wait())
    }

    fn present_ui_with_overlay_once(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
        overlay: &[UiPreparedDraw],
        service_native: &mut impl FnMut(&crate::device::GpuCompletion<'_>) -> Result<(), VulkanError>,
    ) -> Result<UiFrameReport, VulkanError> {
        let report = self.ui_frames.present_composite(
            UiFrameContext {
                gpu_completion: self.gpu_completion.as_mut(),
                device: &self.device,
                capture: self
                    .capture
                    .as_ref()
                    .filter(|capture| !capture.captured)
                    .or_else(|| {
                        self.video_capture
                            .as_ref()
                            .and_then(|video| video.pending())
                    }),
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                pipelines: &self.ui_pipelines,
                meshes: &self.ui_meshes,
                texture_sets: &self.ui_texture_sets,
            },
            logical_extent,
            draws,
            overlay,
            service_native,
        )?;
        self.is_idle = false;
        self.report.presented_ui_draw_count = Some(report.draw_count());
        Ok(report)
    }
}
