//! Scoped presentation ownership and native servicing.

use super::VulkanRenderer;
use crate::device::VulkanError;
use crate::device::vulkan_frame::{CinematicFrameIdentity, FrameContext, FrameUiContext};
use crate::device::vulkan_ui_draw::UiPreparedDraw;

impl VulkanRenderer {
    /// Fits and presents one tightly packed RGBA8 frame.
    ///
    /// This direct pixel boundary serves decoded cinematics without claiming a
    /// texture-cache identity or retaining the caller's frame allocation.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the source extent or byte count is invalid,
    /// or when staging, command submission, synchronization, or presentation
    /// fails.
    pub fn present_rgba8(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(source_extent, rgba8, None, None, &mut |pending| {
                pending.wait()
            })
        })
    }

    /// Presents one retained authored movie frame with linear hardware scaling.
    ///
    /// Repeated calls with the same identity reuse the device-local decoded
    /// image while still presenting at the display's FIFO cadence.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for malformed pixels or Vulkan failures.
    pub fn present_cinematic_rgba8(
        &mut self,
        identity: CinematicFrameIdentity,
        source_extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(
                source_extent,
                rgba8,
                Some(identity),
                None,
                &mut |pending| pending.wait(),
            )
        })
    }

    /// Fits one tightly packed RGBA8 frame and composites retained UI over it.
    ///
    /// This is the movie presentation boundary: decoded pixels retain the
    /// direct transfer path while process-wide overlays are blended before the
    /// acquired swapchain image enters presentation layout.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for invalid source or logical extents, malformed
    /// pixels, stale UI resources, or Vulkan presentation failures.
    pub fn present_rgba8_with_ui(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<(), VulkanError> {
        if logical_extent
            .iter()
            .any(|extent| !extent.is_finite() || *extent <= 0.0)
        {
            return Err(VulkanError::UiFrameExtent);
        }
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(
                source_extent,
                rgba8,
                None,
                Some((logical_extent, draws)),
                &mut |pending| pending.wait(),
            )
        })?;
        self.report.presented_ui_draw_count = Some(draws.len());
        Ok(())
    }

    /// Presents one retained authored movie frame and its process-wide UI.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for invalid extents, malformed pixels, stale UI
    /// resources, or Vulkan presentation failures.
    pub fn present_cinematic_rgba8_with_ui(
        &mut self,
        identity: CinematicFrameIdentity,
        source_extent: (u32, u32),
        rgba8: &[u8],
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<(), VulkanError> {
        if logical_extent
            .iter()
            .any(|extent| !extent.is_finite() || *extent <= 0.0)
        {
            return Err(VulkanError::UiFrameExtent);
        }
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(
                source_extent,
                rgba8,
                Some(identity),
                Some((logical_extent, draws)),
                &mut |pending| pending.wait(),
            )
        })?;
        self.report.presented_ui_draw_count = Some(draws.len());
        Ok(())
    }

    /// Presents an authored movie frame while servicing native acquisition waits.
    /// Optional UI remains in its original order after the decoded movie image.
    ///
    /// # Errors
    /// Returns pixel/UI validation, native servicing, or Vulkan presentation errors.
    pub fn present_cinematic_serviced(
        &mut self,
        identity: CinematicFrameIdentity,
        source_extent: (u32, u32),
        rgba8: &[u8],
        ui: Option<([f32; 2], &[UiPreparedDraw])>,
        service_native: &mut impl FnMut(&crate::device::GpuCompletion<'_>) -> Result<(), VulkanError>,
    ) -> Result<(), VulkanError> {
        if ui.is_some_and(|(extent, _)| {
            extent
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        }) {
            return Err(VulkanError::UiFrameExtent);
        }
        self.with_swapchain_retry_context(
            service_native,
            |renderer, service_native| {
                renderer.present_rgba8_once(
                    source_extent,
                    rgba8,
                    Some(identity),
                    ui,
                    service_native,
                )
            },
            |service_native, pending| service_native(pending),
        )?;
        if let Some((_, draws)) = ui {
            self.report.presented_ui_draw_count = Some(draws.len());
        }
        Ok(())
    }

    /// Records one validated cinematic frame and its optional UI layer.
    fn present_rgba8_once(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
        identity: Option<CinematicFrameIdentity>,
        ui: Option<([f32; 2], &[UiPreparedDraw])>,
        service_native: &mut impl FnMut(&crate::device::GpuCompletion<'_>) -> Result<(), VulkanError>,
    ) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let uploaded = self.cinematic_frames.present(
            FrameContext {
                gpu_completion: self.gpu_completion.as_mut(),
                device: &self.device,
                allocator,
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
                frame_extent: self.report.extent,
                source_extent,
                rgba8,
                identity,
                ui: ui.map(|(logical_extent, draws)| FrameUiContext {
                    logical_extent,
                    pipelines: &self.ui_pipelines,
                    meshes: &self.ui_meshes,
                    texture_sets: &self.ui_texture_sets,
                    draws,
                }),
            },
            service_native,
        )?;
        self.report.presented_source_reused = Some(!uploaded);
        self.is_idle = false;
        Ok(())
    }
}
