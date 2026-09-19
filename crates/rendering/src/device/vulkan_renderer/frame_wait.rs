//! Scoped native servicing while the exact next GPU frame slot is unavailable.

use std::sync::Arc;

use solarity_cpu::CoordinatorNotifier;

use super::VulkanRenderer;
use crate::device::gpu_completion::GpuCompletionService;
use crate::device::{CinematicFrameIdentity, GpuCompletion, VulkanError};

/// Independent presentation rings; waiting one never drains the other rings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuFrameKind {
    /// Unified world and Glue scene presentation.
    World,
    /// UI-only and loading-screen presentation.
    Ui,
    /// Decoded cinematic presentation.
    Cinematic,
    /// Standalone model presentation.
    M2,
    /// Standalone terrain presentation.
    Terrain,
}

impl VulkanRenderer {
    /// Services native input while readers of a changing movie source finish.
    /// The exclusive renderer borrow pins all fences and the shared image until
    /// host observation ends, including callback failure or unwind. Repeating an
    /// unchanged movie frame does not wait for unrelated presentation slots.
    ///
    /// # Errors
    /// Returns configuration, fence observation, or native service failures.
    pub fn wait_for_cinematic_source<E: From<VulkanError>>(
        &mut self,
        extent: (u32, u32),
        identity: CinematicFrameIdentity,
        service_native: impl FnMut(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let completion = self.gpu_completion.as_mut().ok_or_else(|| {
            VulkanError::operation(
                "wait for cinematic source",
                "native waits are not configured",
            )
        })?;
        // Reader checks can return immediately; pending callbacks can service
        // input. Actual host/OS waits have their own narrower instrumentation.
        let _profile = solarity_profiling::profile!("rendering.cinematic_source.reader_check");
        completion.wait_for_pending(
            self.cinematic_frames.source_readers(extent, identity),
            |fence| {
                // SAFETY: The exclusive renderer borrow excludes submission,
                // reset and teardown until every host observer has returned.
                unsafe { self.device.get_fence_status(fence) }.map_err(|source| {
                    VulkanError::operation("query cinematic source reader", source)
                })
            },
            service_native,
        )
    }

    /// Starts the renderer-owned host-wait service before native presentation.
    /// Offline renderers retain explicit synchronous presentation. Configuration
    /// does not change submission order or add a frame of latency.
    ///
    /// # Errors
    /// Returns an error if already configured or the thread cannot start.
    pub fn configure_frame_waits(
        &mut self,
        notifier: Arc<dyn CoordinatorNotifier>,
    ) -> Result<(), VulkanError> {
        if self.gpu_completion.is_some() {
            return Err(VulkanError::operation(
                "configure GPU frame waits",
                "completion service is already configured",
            ));
        }
        let device = self.device.clone();
        self.gpu_completion = Some(GpuCompletionService::new(
            move |fence| {
                // SAFETY: wait_for_frame_slot keeps the renderer exclusively
                // borrowed until host completion, including error/unwind drain.
                // Submission has returned; reset/destroy cannot run concurrently.
                // Renderer drop joins this thread before destroying the device.
                unsafe { device.wait_for_fences(&[fence], true, u64::MAX) }
            },
            notifier,
        )?);
        Ok(())
    }

    /// Services the native coordinator only if this ring's next slot is pending.
    /// Call immediately before presentation, after independent CPU preparation.
    /// The callback can observe readiness but cannot mutate this renderer. A
    /// callback error or unwind still drains the host observer before returning
    /// ownership. Existing presentation validates/resets the slot normally.
    ///
    /// # Errors
    /// Returns configuration, Vulkan completion, or native service errors.
    pub fn wait_for_frame_slot<E: From<VulkanError>>(
        &mut self,
        kind: GpuFrameKind,
        service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let completion = self.gpu_completion.as_mut().ok_or_else(|| {
            VulkanError::operation("wait for GPU frame slot", "native waits are not configured")
        })?;
        completion.check_health()?;
        let fence = match kind {
            GpuFrameKind::World => self.world_frames.pending_fence(),
            GpuFrameKind::Ui => self.ui_frames.pending_fence(),
            GpuFrameKind::Cinematic => self.cinematic_frames.pending_fence(),
            GpuFrameKind::M2 => self.m2_frames.pending_fence(),
            GpuFrameKind::Terrain => self.terrain_frames.pending_fence(),
        };
        let Some(fence) = fence else {
            return Ok(());
        };
        // SAFETY: The next slot is live, and this exclusive renderer borrow
        // excludes queue submission, fence reset and resource destruction.
        if unsafe { self.device.get_fence_status(fence) }
            .map_err(|source| VulkanError::operation("query GPU frame slot", source))?
        {
            return Ok(());
        }
        let _profile = solarity_profiling::profile!("rendering.gpu_slot.pending");
        completion.wait_for(fence, service_native)
    }
}
