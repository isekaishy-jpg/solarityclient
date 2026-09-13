//! On-demand accounting; no per-frame registry traversal or diagnostic allocation.

use super::super::VulkanRenderer;

/// Payload and object accounting for retained model/UI resources. Bytes describe
/// allocated content capacity, excluding driver metadata and opaque VMA padding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuResourceUsage {
    /// Live M2 geometry resources and their vertex/index payload bytes.
    pub m2_meshes: (usize, usize),
    /// Live UI meshes and retained vertex/index capacities.
    pub ui_meshes: (usize, usize),
    /// Live common glyph pages and their RGBA bytes.
    pub glyph_pages: (usize, usize),
    /// Live character recipes and their complete mip payload bytes.
    pub character_atlases: (usize, usize),
    /// Authored/common BLP resources retain their separate path cache policy.
    pub common_textures: (usize, usize),
    /// Live UI sampled-image descriptor sets.
    pub ui_texture_sets: usize,
    /// Live M2 material descriptor sets.
    pub m2_texture_sets: usize,
    /// CPU-invalidated resource batches still waiting on GPU completion.
    pub pending_retirement_batches: usize,
}

impl VulkanRenderer {
    /// Returns explicit resource accounting for diagnostics and lifecycle tests.
    /// This on-demand traversal is never called by the ordinary frame loop.
    pub fn resource_usage(&self) -> GpuResourceUsage {
        GpuResourceUsage {
            m2_meshes: self.m2_meshes.usage(),
            ui_meshes: self.ui_meshes.usage(),
            glyph_pages: self.ui_glyph_textures.usage(),
            character_atlases: self.character_atlas_textures.usage(),
            common_textures: self.blp_textures.usage(),
            ui_texture_sets: self.ui_texture_sets.resource_count(),
            m2_texture_sets: self.m2_texture_sets.resource_count(),
            pending_retirement_batches: self.resource_lifetimes.pending.len(),
        }
    }
}
