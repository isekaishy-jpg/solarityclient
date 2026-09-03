//! Cache identities and owned SPIR-V words for one M2 effect pair.

use super::super::{M2ShaderPermutation, M2ShaderPlan};

/// Complete immutable identity of one compiled M2 shader pair.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2SpirvKey {
    plan: M2ShaderPlan,
    permutation: M2ShaderPermutation,
}

impl M2SpirvKey {
    /// Combines effect substitution, material state, and BLS indices.
    #[must_use]
    pub const fn new(plan: M2ShaderPlan, permutation: M2ShaderPermutation) -> Self {
        Self { plan, permutation }
    }

    /// Returns the stock effect and immutable material selection.
    #[must_use]
    pub const fn plan(self) -> M2ShaderPlan {
        self.plan
    }

    /// Returns the exact vertex and pixel permutation indices.
    #[must_use]
    pub const fn permutation(self) -> M2ShaderPermutation {
        self.permutation
    }
}

/// Owned Vulkan shader bytecode ready for `vkCreateShaderModule`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M2SpirvProgram {
    key: M2SpirvKey,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
    vertex_specialization: [u32; 2],
    fragment_specialization: [u32; 3],
}

impl M2SpirvProgram {
    /// Retains a compiled pair under its complete pipeline-cache identity.
    pub(super) fn new(
        key: M2SpirvKey,
        vertex_words: Vec<u32>,
        fragment_words: Vec<u32>,
        vertex_specialization: [u32; 2],
        fragment_specialization: [u32; 3],
    ) -> Self {
        Self {
            key,
            vertex_words,
            fragment_words,
            vertex_specialization,
            fragment_specialization,
        }
    }

    /// Returns the immutable compilation identity.
    #[must_use]
    pub const fn key(&self) -> M2SpirvKey {
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

    /// Returns the coordinate-effect and vertex-permutation constants.
    #[must_use]
    pub const fn vertex_specialization(&self) -> [u32; 2] {
        self.vertex_specialization
    }

    /// Returns the combiner, pixel-permutation, and texture-count constants.
    #[must_use]
    pub const fn fragment_specialization(&self) -> [u32; 3] {
        self.fragment_specialization
    }
}
