//! Borrowed CPU-only resource catalog for joined effect packet preparation.

use super::VulkanRenderer;
use crate::device::vulkan_m2_particle_pipeline::M2ParticlePipelineRegistry;
use crate::device::vulkan_m2_ribbon_pipeline::M2RibbonPipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::{
    M2EffectOrder, M2ParticlePipelineHandle, M2ParticlePreparedDraw, M2RibbonPipelineHandle,
    M2RibbonPreparedDraw, M2TextureSetHandle, VulkanError,
};

/// Immutable validation tables borrowed while no GPU resource registry can mutate.
/// Contains no device, queue, allocation owner or command recorder.
#[derive(Clone, Copy)]
pub struct M2EffectDrawCatalog<'a> {
    particles: &'a M2ParticlePipelineRegistry,
    ribbons: &'a M2RibbonPipelineRegistry,
    textures: &'a M2TextureSetRegistry,
}

impl VulkanRenderer {
    /// Borrows CPU-only tables for joined packet preparation on frame workers.
    #[must_use]
    pub fn m2_effect_draw_catalog(&self) -> M2EffectDrawCatalog<'_> {
        M2EffectDrawCatalog {
            particles: &self.m2_particle_pipelines,
            ribbons: &self.m2_ribbon_pipelines,
            textures: &self.m2_texture_sets,
        }
    }
}

impl M2EffectDrawCatalog<'_> {
    /// Validates one dynamic emitter range against the retained resource image.
    /// # Errors
    /// Returns the ordinary particle resource, material and range errors.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_m2_particle_draw_range(
        &self,
        pipeline: M2ParticlePipelineHandle,
        texture_set: M2TextureSetHandle,
        blending_type: u8,
        particle_flags: u32,
        element_alpha: f32,
        order: M2EffectOrder,
        first_vertex: u32,
        first_index: u32,
        vertex_count: usize,
        index_count: usize,
    ) -> Result<M2ParticlePreparedDraw, VulkanError> {
        crate::device::vulkan_m2_particle_draw::prepare_draw(
            self.particles,
            self.textures,
            pipeline,
            texture_set,
            blending_type,
            particle_flags,
            element_alpha,
            order,
            first_vertex,
            first_index,
            vertex_count,
            index_count,
        )
    }

    /// Validates one ribbon range against the same retained resource image.
    /// # Errors
    /// Returns the ordinary ribbon resource, material and range errors.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_m2_ribbon_draw_range(
        &self,
        pipeline: M2RibbonPipelineHandle,
        texture_set: M2TextureSetHandle,
        material: solarity_asset::M2Material,
        order: M2EffectOrder,
        first_vertex: u32,
        vertex_count: usize,
    ) -> Result<M2RibbonPreparedDraw, VulkanError> {
        crate::device::vulkan_m2_ribbon_draw::prepare_draw(
            self.ribbons,
            self.textures,
            pipeline,
            texture_set,
            material,
            order,
            first_vertex,
            vertex_count,
        )
    }
}
