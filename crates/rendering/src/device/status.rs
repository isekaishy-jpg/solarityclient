//! Stable failures for Vulkan initialization and teardown.

use thiserror::Error;

use solarity_asset::AssetPath;

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
    /// Swapchain images cannot receive the bootstrap texture transfer.
    #[error("surface images do not support transfer-destination usage")]
    TransferDestinationUsage,
    /// CPU frame composition overflowed addressable memory.
    #[error("bootstrap frame dimensions exceed addressable memory")]
    FrameSize,
    /// A decoded M2 has no geometry that Vulkan can bind and draw.
    #[error("M2 mesh {path} has no {buffer_kind} data to upload")]
    EmptyM2Mesh {
        /// Model whose selected profile produced the empty buffer.
        path: AssetPath,
        /// Stable name of the absent vertex or index payload.
        buffer_kind: &'static str,
    },
    /// The renderer cannot assign another stable 32-bit mesh handle.
    #[error("M2 mesh registry exhausted its 32-bit handle space")]
    M2MeshCapacity,
    /// The stock M2 shader pair could not be translated to the pinned target.
    #[error("M2 shader preparation failed: {message}")]
    M2Shader {
        /// Stable shader compiler or unsupported-permutation diagnostic.
        message: String,
    },
    /// The renderer cannot assign another stable 32-bit pipeline handle.
    #[error("M2 pipeline registry exhausted its 32-bit handle space")]
    M2PipelineCapacity,
    /// The renderer cannot assign another stable 32-bit sampler handle.
    #[error("M2 sampler registry exhausted its 32-bit handle space")]
    M2SamplerCapacity,
    /// A sampled image handle belongs to another renderer or no live image.
    #[error("M2 texture set references an unknown BLP texture handle")]
    UnknownBlpTextureHandle,
    /// A sampler handle belongs to another renderer or no live sampler.
    #[error("M2 texture set references an unknown M2 sampler handle")]
    UnknownM2SamplerHandle,
    /// The renderer cannot assign another stable 32-bit texture-set handle.
    #[error("M2 texture-set registry exhausted its 32-bit handle space")]
    M2TextureSetCapacity,
    /// A mesh handle belongs to another renderer or no live allocation.
    #[error("M2 draw references an unknown mesh handle")]
    UnknownM2MeshHandle,
    /// A pipeline handle belongs to another renderer or no live pipeline.
    #[error("M2 draw references an unknown pipeline handle")]
    UnknownM2PipelineHandle,
    /// A texture-set handle belongs to another renderer or no live set.
    #[error("M2 draw references an unknown texture-set handle")]
    UnknownM2TextureSetHandle,
    /// The uploaded geometry and CPU plan identify different model profiles.
    #[error("M2 draw mesh does not match its CPU mesh plan")]
    M2DrawMeshMismatch,
    /// The requested material batch does not exist in the CPU plan.
    #[error("M2 draw index {requested} is unavailable; plan has {available} draws")]
    M2DrawIndex {
        /// Requested zero-based material batch.
        requested: usize,
        /// Number of available plan draws.
        available: usize,
    },
    /// The draw's index span exceeds its uploaded geometry allocation.
    #[error("M2 draw index range exceeds the uploaded mesh")]
    M2DrawIndexRange,
    /// The compiled pipeline does not represent the draw's material state.
    #[error("M2 draw pipeline does not match its material batch")]
    M2DrawPipelineMismatch,
    /// The sampled texture stages do not match the compiled shader count.
    #[error("M2 draw texture set does not match its pipeline")]
    M2DrawTextureSetMismatch,
    /// The renderer cannot assign another stable 32-bit texture handle.
    #[error("BLP texture registry exhausted its 32-bit handle space")]
    BlpTextureCapacity,
    /// The selected adapter lacks the pinned stock-compatible depth format.
    #[error("selected Vulkan adapter lacks D24_UNORM_S8_UINT depth/stencil attachments")]
    DepthStencilFormat,
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
