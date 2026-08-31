//! Owned ribbon SPIR-V words and their complete material identity.

use crate::M2MaterialState;

/// One material-specialized ribbon shader pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M2RibbonSpirvProgram {
    material: M2MaterialState,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl M2RibbonSpirvProgram {
    pub(super) fn new(
        material: M2MaterialState,
        vertex_words: Vec<u32>,
        fragment_words: Vec<u32>,
    ) -> Self {
        Self {
            material,
            vertex_words,
            fragment_words,
        }
    }

    /// Returns the fixed material state encoded into the fragment variant.
    #[must_use]
    pub const fn material(&self) -> M2MaterialState {
        self.material
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
