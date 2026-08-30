//! Cache identity and owned module words for one MapObj effect pair.

use solarity_asset::WorldModelShader;

use super::WorldModelSpirvError;

/// Complete shader-module identity for an ordinary or unified effect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSpirvKey {
    shader: WorldModelShader,
    unified: bool,
}

impl WorldModelSpirvKey {
    /// Validates one selector against stock's two seven-entry effect tables.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelSpirvError::OrdinaryComposite`] because selector
    /// six is a null entry in the non-unified MapObj table.
    pub const fn new(
        shader: WorldModelShader,
        unified: bool,
    ) -> Result<Self, WorldModelSpirvError> {
        if !unified && matches!(shader, WorldModelShader::Composite) {
            return Err(WorldModelSpirvError::OrdinaryComposite);
        }
        Ok(Self { shader, unified })
    }

    /// Returns the normalized MOMT effect selector.
    #[must_use]
    pub const fn shader(self) -> WorldModelShader {
        self.shader
    }

    /// Reports whether this is a MapObjU effect.
    #[must_use]
    pub const fn is_unified(self) -> bool {
        self.unified
    }
}

/// Owned Vulkan shader bytecode ready for module creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldModelSpirvProgram {
    key: WorldModelSpirvKey,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl WorldModelSpirvProgram {
    pub(super) fn new(
        key: WorldModelSpirvKey,
        vertex_words: Vec<u32>,
        fragment_words: Vec<u32>,
    ) -> Self {
        Self {
            key,
            vertex_words,
            fragment_words,
        }
    }

    /// Returns the exact ordinary/unified effect identity.
    #[must_use]
    pub const fn key(&self) -> WorldModelSpirvKey {
        self.key
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
