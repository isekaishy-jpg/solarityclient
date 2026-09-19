//! Native GPU-slot service shares the CPU coordinator's durable wake protocol.

use solarity_rendering::{GpuFrameKind, VulkanRenderer};

use super::FrameWait;

/// GPU waits retain their driver/native failure without involving CPU job errors.
#[derive(Debug, thiserror::Error)]
pub(in crate::application) enum GpuFrameWaitError {
    #[error(transparent)]
    Vulkan(#[from] solarity_rendering::VulkanError),
    #[error(transparent)]
    Platform(#[from] crate::platform::PlatformError),
}

impl FrameWait<'_> {
    /// A new authored movie frame replaces one image shared by every slot. Drain
    /// those readers through the native bridge before the existing upload owner
    /// writes pixels; decoded timing and audio selection remain unchanged.
    pub(in crate::application) fn before_cinematic_source(
        &mut self,
        renderer: &mut VulkanRenderer,
        extent: (u32, u32),
        identity: solarity_rendering::CinematicFrameIdentity,
    ) -> Result<(), GpuFrameWaitError> {
        let Self::Native(platform) = self else {
            return Ok(());
        };
        renderer.wait_for_cinematic_source(extent, identity, |completion| {
            platform.wait_until_ready(|| Ok(completion.is_ready()))
        })
    }

    /// Leaves all gameplay events queued and retains renderer ownership until
    /// its external host wait has released the fence. Offline rendering uses
    /// the existing synchronous wait inside presentation.
    pub(in crate::application) fn before_gpu_frame(
        &mut self,
        renderer: &mut VulkanRenderer,
        kind: GpuFrameKind,
    ) -> Result<(), GpuFrameWaitError> {
        let Self::Native(platform) = self else {
            return Ok(());
        };
        renderer.wait_for_frame_slot(kind, |completion| {
            platform.wait_until_ready(|| Ok(completion.is_ready()))
        })
    }
}
