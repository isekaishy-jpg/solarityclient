//! Shared acquisition boundary for native presentation and explicit offline rendering.

#![allow(unsafe_code)]

use ash::vk;

use super::{GpuCompletion, GpuCompletionService};
use crate::device::{VulkanError, vulkan_frame::swapchain_error};

/// Exclusive renderer ownership pins both handles, excludes submission/recreation,
/// and retains the selected slot until the host operation drains on every exit.
pub(in crate::device) fn acquire_image(
    loader: &ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    semaphore: vk::Semaphore,
    completion: Option<&mut GpuCompletionService>,
    service_native: impl FnOnce(&GpuCompletion<'_>) -> Result<(), VulkanError>,
) -> Result<(u32, bool), VulkanError> {
    let acquire = |timeout| {
        // SAFETY: Callers retain exclusive renderer ownership and an unsignaled
        // semaphore from the completed selected slot throughout this operation.
        unsafe { loader.acquire_next_image(swapchain, timeout, semaphore, vk::Fence::null()) }
    };
    match completion {
        Some(completion) => {
            completion.acquire_when_pending(swapchain, semaphore, || acquire(0), service_native)
        }
        None => acquire(u64::MAX).map_err(|source| swapchain_error("acquire frame image", source)),
    }
}
