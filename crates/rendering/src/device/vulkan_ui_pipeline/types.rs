//! Stable UI pipeline identity and observable immutable material state.

use crate::{UiRenderBlend, UiShaderSource};

/// Stable renderer-local handle to one Vulkan UI graphics pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiPipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Source and framebuffer operation represented by one live pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiPipelineInfo {
    source: UiShaderSource,
    blend: UiRenderBlend,
}

impl UiPipelineInfo {
    /// Captures the complete immutable simple-render pipeline selection.
    pub(super) const fn new(source: UiShaderSource, blend: UiRenderBlend) -> Self {
        Self { source, blend }
    }

    /// Returns whether the fragment stage samples an image.
    #[must_use]
    pub const fn source(self) -> UiShaderSource {
        self.source
    }

    /// Returns the fixed framebuffer blend operation.
    #[must_use]
    pub const fn blend(self) -> UiRenderBlend {
        self.blend
    }
}
