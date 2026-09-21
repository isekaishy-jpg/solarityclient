//! Typed host work retains Vulkan handle identity through one scoped observation.

use ash::vk;

/// Handles stay pinned by the exclusive renderer borrow until host completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::device) enum HostOperation {
    /// Observe submission completion without resetting or submitting a fence.
    Fence(vk::Fence),
    /// Retire all device users at an existing surface/shared-resource boundary.
    DeviceIdle,
    /// Acquire exactly one image using the selected frame slot's semaphore.
    Acquire {
        swapchain: vk::SwapchainKHR,
        semaphore: vk::Semaphore,
    },
}

impl HostOperation {
    /// Stable failure context identifies the actual host dependency.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Fence(_) => "wait for GPU frame slot",
            Self::DeviceIdle => "wait for GPU resource retirement",
            Self::Acquire { .. } => "acquire frame image",
        }
    }
}

/// Durable output has no borrowed driver storage or mutable resource access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::device) enum HostOutput {
    /// The fence or device-idle observation reached its terminal host result.
    Complete,
    /// Preserve both the image index and the driver's suboptimal indication.
    Acquired { index: u32, suboptimal: bool },
}
