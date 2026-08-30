//! Stable failures for Vulkan initialization and teardown.

use thiserror::Error;

/// A failure to establish or stop the required Vulkan 1.3 presentation stack.
#[derive(Debug, Error)]
pub enum VulkanError {
    /// The Vulkan loader could not be opened on this system.
    #[error("failed to load Vulkan: {message}")]
    Load {
        /// Loader diagnostic text.
        message: String,
    },
    /// The loader does not expose the pinned Vulkan API level.
    #[error("Vulkan 1.3 is required, loader reports {major}.{minor}.{patch}")]
    UnsupportedApi {
        /// Reported major version.
        major: u32,
        /// Reported minor version.
        minor: u32,
        /// Reported patch version.
        patch: u32,
    },
    /// An SDL-required instance extension contained an interior NUL byte.
    #[error("invalid Vulkan instance extension {extension}")]
    InvalidExtension {
        /// Rejected extension name.
        extension: String,
    },
    /// A Vulkan call failed during a named initialization or shutdown phase.
    #[error("Vulkan operation {operation} failed: {message}")]
    Operation {
        /// Stable operation name for programmatic diagnosis.
        operation: &'static str,
        /// Vulkan result rendered without exposing Ash types.
        message: String,
    },
    /// The explicit zero-based adapter index does not exist.
    #[error("Vulkan adapter index {requested} is unavailable; found {available} adapters")]
    AdapterUnavailable {
        /// Explicit index supplied by runtime configuration.
        requested: usize,
        /// Number of adapters enumerated by Vulkan.
        available: usize,
    },
    /// The selected physical device does not implement Vulkan 1.3.
    #[error("selected Vulkan adapter reports API {major}.{minor}.{patch}, but 1.3 is required")]
    AdapterApi {
        /// Reported major version.
        major: u32,
        /// Reported minor version.
        minor: u32,
        /// Reported patch version.
        patch: u32,
    },
    /// No graphics and presentation queue arrangement exists for the surface.
    #[error("selected Vulkan adapter has no compatible graphics/presentation queues")]
    QueueFamilies,
    /// The selected adapter cannot present through `VK_KHR_swapchain`.
    #[error("selected Vulkan adapter does not expose VK_KHR_swapchain")]
    SwapchainExtension,
    /// Required Vulkan 1.3 rendering primitives are unavailable.
    #[error("selected Vulkan adapter lacks dynamic rendering or synchronization2")]
    Vulkan13Features,
    /// The surface does not expose the stock-compatible BGRA8 format.
    #[error("surface does not expose B8G8R8A8_UNORM with SRGB_NONLINEAR color space")]
    SurfaceFormat,
    /// FIFO presentation, required by this deterministic bootstrap, is absent.
    #[error("surface does not expose FIFO presentation")]
    PresentMode,
    /// The surface cannot be presented as an opaque desktop window.
    #[error("surface does not support opaque composition")]
    CompositeAlpha,
    /// Swapchain images cannot be used as color attachments.
    #[error("surface images do not support color-attachment usage")]
    ColorAttachmentUsage,
}

impl VulkanError {
    /// Adds a stable operation label to an Ash/Vulkan diagnostic.
    pub(super) fn operation(operation: &'static str, source: impl ToString) -> Self {
        Self::Operation {
            operation,
            message: source.to_string(),
        }
    }
}
