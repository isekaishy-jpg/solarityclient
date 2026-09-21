//! Scoped world host waits share the existing renderer completion thread.

#![allow(unsafe_code)]

use super::{WorldFrameContext, WorldFrameExecution};
use crate::device::{VulkanError, gpu_completion::acquire_image};
use ash::vk;

impl WorldFrameContext<'_> {
    /// Probe once, then service native input while the driver acquires an image.
    /// The context's exclusive renderer borrow prevents recreation or submission
    /// until the service drains, including when native servicing fails or unwinds.
    pub(super) fn acquire(
        &mut self,
        execution: &mut impl WorldFrameExecution,
        semaphore: vk::Semaphore,
    ) -> Result<(u32, bool), VulkanError> {
        acquire_image(
            self.swapchain_loader,
            self.swapchain,
            semaphore,
            self.gpu_completion.as_deref_mut(),
            |pending| execution.wait_for_gpu(pending),
        )
    }

    /// Keeps the existing rare rebuild/quality lifetime barrier, with native input
    /// service throughout. Ordinary selected-slot buffer growth does not enter it.
    pub(super) fn wait_idle(
        &mut self,
        execution: &mut impl WorldFrameExecution,
    ) -> Result<(), VulkanError> {
        if let Some(completion) = self.gpu_completion.as_deref_mut() {
            completion.idle(|pending| execution.wait_for_gpu(pending))
        } else {
            // SAFETY: Explicit offline presentation exclusively owns renderer
            // submission and resource destruction across this existing barrier.
            unsafe { self.device.device_wait_idle() }.map_err(|source| {
                VulkanError::operation("wait for GPU resource retirement", source)
            })
        }
    }
}
