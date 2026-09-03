//! Validated dynamic particle draw packets for unified world recording.

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2SceneLightBank;
use crate::device::vulkan_m2_particle_pipeline::{
    M2ParticlePipelineHandle, M2ParticlePipelineRegistry,
};
use crate::device::vulkan_m2_texture_set::{M2TextureSetHandle, M2TextureSetRegistry};
use crate::{M2EffectOrder, M2MaterialState};

/// Renderer-local resources and indexed ranges for one particle emitter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2ParticlePreparedDraw {
    pipeline: M2ParticlePipelineHandle,
    texture_set: M2TextureSetHandle,
    vertex_offset: i32,
    first_index: u32,
    index_count: u32,
    light_bank: M2SceneLightBank,
    order: M2EffectOrder,
    blend_order: u8,
}

impl M2ParticlePreparedDraw {
    /// Assigns the stock Glue light bank used by unified-frame presentation.
    #[must_use]
    pub const fn with_light_bank(mut self, light_bank: M2SceneLightBank) -> Self {
        self.light_bank = light_bank;
        self
    }

    /// Returns the scene-light bank selected for this emitter instance.
    #[must_use]
    pub const fn light_bank(self) -> M2SceneLightBank {
        self.light_bank
    }

    pub(in crate::device) const fn pipeline(self) -> M2ParticlePipelineHandle {
        self.pipeline
    }

    pub(in crate::device) const fn texture_set(self) -> M2TextureSetHandle {
        self.texture_set
    }

    /// Returns the signed base added to each emitter-local index.
    #[must_use]
    pub const fn vertex_offset(self) -> i32 {
        self.vertex_offset
    }

    /// Returns the first index in the frame's concatenated dynamic stream.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the exact triangle-list index count.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Returns the authored plane used for stock mesh/effect interleaving.
    #[must_use]
    pub const fn priority_plane(self) -> i16 {
        self.order.priority_plane()
    }

    /// Returns the authored blend discriminator used within one effect plane.
    #[must_use]
    pub const fn blend_order(self) -> u8 {
        self.blend_order
    }

    /// Returns stable producer order after plane and blend comparisons tie.
    #[must_use]
    pub const fn effect_order(self) -> u32 {
        self.order.producer_order()
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    pipelines: &M2ParticlePipelineRegistry,
    texture_sets: &M2TextureSetRegistry,
    pipeline: M2ParticlePipelineHandle,
    texture_set: M2TextureSetHandle,
    blending_type: u8,
    particle_flags: u32,
    order: M2EffectOrder,
    first_vertex: u32,
    first_index: u32,
    vertex_count: usize,
    index_count: usize,
) -> Result<M2ParticlePreparedDraw, VulkanError> {
    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownM2ParticlePipelineHandle)?;
    if pipeline_info.material() != M2MaterialState::from_particle(blending_type, particle_flags) {
        return Err(VulkanError::M2ParticleDrawPipelineMismatch);
    }
    let texture_info = texture_sets
        .info(texture_set)
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    if texture_info.stage_count() != 1 {
        return Err(VulkanError::M2ParticleDrawTextureSetMismatch);
    }
    let vertex_count =
        u32::try_from(vertex_count).map_err(|_source| VulkanError::M2ParticleDrawVertexRange)?;
    first_vertex
        .checked_add(vertex_count)
        .ok_or(VulkanError::M2ParticleDrawVertexRange)?;
    let vertex_offset =
        i32::try_from(first_vertex).map_err(|_source| VulkanError::M2ParticleDrawVertexRange)?;
    let index_count =
        u32::try_from(index_count).map_err(|_source| VulkanError::M2ParticleDrawIndexRange)?;
    first_index
        .checked_add(index_count)
        .ok_or(VulkanError::M2ParticleDrawIndexRange)?;
    Ok(M2ParticlePreparedDraw {
        pipeline,
        texture_set,
        vertex_offset,
        first_index,
        index_count,
        light_bank: M2SceneLightBank::Environment,
        order,
        blend_order: blending_type,
    })
}
