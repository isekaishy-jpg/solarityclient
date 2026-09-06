//! UI fragment-source identity and owned SPIR-V module words.

/// Closed fragment-source domain used by stock simple textured regions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiShaderSource {
    /// Sample one bound BLP and multiply it by the interpolated corner color.
    Texture,
    /// Sample a source image and an independently positioned alpha mask.
    MaskedTexture,
    /// Emit interpolated corner color without requiring an image descriptor.
    VertexColor,
}

/// Owned Vulkan shader bytecode for one UI source variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSpirvProgram {
    source: UiShaderSource,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl UiSpirvProgram {
    /// Retains a compiled pair under its complete shader identity.
    pub(super) fn new(
        source: UiShaderSource,
        vertex_words: Vec<u32>,
        fragment_words: Vec<u32>,
    ) -> Self {
        Self {
            source,
            vertex_words,
            fragment_words,
        }
    }

    /// Returns whether this pair samples a BLP or only vertex color.
    #[must_use]
    pub const fn source(&self) -> UiShaderSource {
        self.source
    }

    /// Returns the vertex module as native SPIR-V words.
    #[must_use]
    pub fn vertex_words(&self) -> &[u32] {
        &self.vertex_words
    }

    /// Returns the fragment module as native SPIR-V words.
    #[must_use]
    pub fn fragment_words(&self) -> &[u32] {
        &self.fragment_words
    }
}
