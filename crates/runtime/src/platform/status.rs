//! Stable errors at the SDL platform boundary.

use thiserror::Error;

/// A failure while owning the primary SDL window and event source.
#[derive(Debug, Error)]
pub enum PlatformError {
    /// SDL process initialization failed.
    #[error("failed to initialize SDL: {message}")]
    Initialize {
        /// SDL's diagnostic text.
        message: String,
    },
    /// SDL could not initialize its video subsystem.
    #[error("failed to initialize SDL video: {message}")]
    Video {
        /// SDL's diagnostic text.
        message: String,
    },
    /// SDL could not construct the configured Vulkan-capable window.
    #[error("failed to create SDL Vulkan window: {message}")]
    Window {
        /// SDL's diagnostic text.
        message: String,
    },
    /// SDL could not grant the process-wide event pump.
    #[error("failed to create SDL event pump: {message}")]
    EventPump {
        /// SDL's diagnostic text.
        message: String,
    },
    /// SDL could not report the instance extensions required by this window.
    #[error("failed to query SDL Vulkan instance extensions: {message}")]
    VulkanExtensions {
        /// SDL's diagnostic text.
        message: String,
    },
    /// SDL could not construct a native Vulkan surface for its window.
    #[error("failed to create SDL Vulkan surface: {message}")]
    VulkanSurface {
        /// SDL's diagnostic text.
        message: String,
    },
}
