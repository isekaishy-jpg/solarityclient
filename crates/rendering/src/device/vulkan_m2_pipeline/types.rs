//! Public typed pipeline identity and immutable effect diagnostics.

use crate::shader::{M2MaterialState, M2PixelShader, M2ShaderPermutation, M2VertexShader};

/// Stable renderer-local handle to one Vulkan M2 graphics pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2PipelineHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable stock effect and BLS indices represented by a live pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2PipelineInfo {
    vertex_shader: M2VertexShader,
    pixel_shader: M2PixelShader,
    texture_count: u16,
    material: M2MaterialState,
    permutation: M2ShaderPermutation,
}

impl M2PipelineInfo {
    /// Captures the stock names and exact permutation indices for diagnostics.
    pub(super) const fn new(
        vertex_shader: M2VertexShader,
        pixel_shader: M2PixelShader,
        texture_count: u16,
        material: M2MaterialState,
        permutation: M2ShaderPermutation,
    ) -> Self {
        Self {
            vertex_shader,
            pixel_shader,
            texture_count,
            material,
            permutation,
        }
    }

    /// Returns the selected stock vertex effect basename.
    #[must_use]
    pub const fn vertex_shader(self) -> M2VertexShader {
        self.vertex_shader
    }

    /// Returns the selected stock pixel effect basename.
    #[must_use]
    pub const fn pixel_shader(self) -> M2PixelShader {
        self.pixel_shader
    }

    /// Returns the exact number of texture descriptors statically consumed.
    #[must_use]
    pub const fn texture_count(self) -> u16 {
        self.texture_count
    }

    /// Returns the fixed-function material state compiled into the pipeline.
    #[must_use]
    pub const fn material(self) -> M2MaterialState {
        self.material
    }

    /// Returns the exact BLS vertex and pixel indices.
    #[must_use]
    pub const fn permutation(self) -> M2ShaderPermutation {
        self.permutation
    }
}
