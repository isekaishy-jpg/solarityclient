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
