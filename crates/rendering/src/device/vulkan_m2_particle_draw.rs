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
    vertex_count: u32,
    first_index: u32,
    index_count: u32,
    light_bank: M2SceneLightBank,
    scene_index: Option<u32>,
    order: M2EffectOrder,
    blend_order: u8,
    scene_order: u32,
    alpha_reference_bits: u32,
}

impl M2ParticlePreparedDraw {
    /// Relocates a worker-local stream and producer order into the joined frame.
    /// # Errors
    /// Returns range overflow before a relocated packet is published.
    pub fn relocate(
        mut self,
        vertices: u32,
        indices: u32,
        scene: u32,
        effects: u32,
    ) -> Result<Self, VulkanError> {
        let first = u32::try_from(self.vertex_offset)
            .ok()
            .and_then(|first| first.checked_add(vertices))
            .ok_or(VulkanError::M2ParticleDrawVertexRange)?;
        first
            .checked_add(self.vertex_count)
            .ok_or(VulkanError::M2ParticleDrawVertexRange)?;
        self.vertex_offset =
            i32::try_from(first).map_err(|_| VulkanError::M2ParticleDrawVertexRange)?;
        self.first_index = self
            .first_index
            .checked_add(indices)
            .ok_or(VulkanError::M2ParticleDrawIndexRange)?;
        self.first_index
            .checked_add(self.index_count)
            .ok_or(VulkanError::M2ParticleDrawIndexRange)?;
        self.order = M2EffectOrder::new(
            self.priority_plane(),
            self.effect_order()
                .checked_add(effects)
                .ok_or(VulkanError::M2ParticleDrawIndexRange)?,
        );
        if self.scene_order != u32::MAX {
            self.scene_order = self
                .scene_order
                .checked_add(scene)
                .ok_or(VulkanError::M2ParticleDrawIndexRange)?;
        }
        Ok(self)
    }

    /// Returns the per-element alpha-test threshold supplied by `81FE90`.
    #[must_use]
    pub const fn alpha_reference(self) -> f32 {
        f32::from_bits(self.alpha_reference_bits)
    }

    /// Selects the world instance scene supplied in `WorldFrameScene`.
    #[must_use]
    pub const fn with_scene_index(mut self, index: Option<u32>) -> Self {
        self.scene_index = index;
        self
    }

    /// Returns the world instance scene, or the fixed Glue/sky bank.
    #[must_use]
    pub const fn scene_index(self) -> Option<u32> {
        self.scene_index
    }

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

    /// Assigns this emitter's position in the unified stock scene-element queue.
    #[must_use]
    pub const fn with_scene_order(mut self, scene_order: u32) -> Self {
        self.scene_order = scene_order;
        self
    }

    /// Returns this emitter's position in the unified scene-element queue.
    #[must_use]
    pub const fn scene_order(self) -> u32 {
        self.scene_order
    }
}

/// Immutable resource compatibility retained by a source's GPU resource lease.
#[derive(Clone, Copy)]
pub struct M2ParticleDrawTemplate {
    pipeline: M2ParticlePipelineHandle,
    texture_set: M2TextureSetHandle,
    material: M2MaterialState,
}

impl M2ParticleDrawTemplate {
    /// Validates resource identity once while the renderer owns its registries.
    pub(in crate::device) fn new(
        pipelines: &M2ParticlePipelineRegistry,
        textures: &M2TextureSetRegistry,
        pipeline: M2ParticlePipelineHandle,
        texture_set: M2TextureSetHandle,
    ) -> Result<Self, VulkanError> {
        let material = pipelines
            .info(pipeline)
            .ok_or(VulkanError::UnknownM2ParticlePipelineHandle)?
            .material();
        let texture = textures
            .info(texture_set)
            .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
        if texture.stage_count() != 1 {
            return Err(VulkanError::M2ParticleDrawTextureSetMismatch);
        }
        Ok(Self {
            pipeline,
            texture_set,
            material,
        })
    }

    /// Builds dynamic ranges without borrowing or rescanning renderer registries.
    /// # Errors
    /// Preserves material mismatch and range overflow validation.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &self,
        blending_type: u8,
        particle_flags: u32,
        element_alpha: f32,
        order: M2EffectOrder,
        first_vertex: u32,
        first_index: u32,
        vertex_count: usize,
        index_count: usize,
    ) -> Result<M2ParticlePreparedDraw, VulkanError> {
        let mut material = M2MaterialState::from_particle(blending_type, particle_flags);
        if !material.blend_enabled() && element_alpha < 0.999_99 {
            material = material.with_runtime_alpha_fade();
        }
        if self.material != material {
            return Err(VulkanError::M2ParticleDrawPipelineMismatch);
        }
        let pipeline = self.pipeline;
        let texture_set = self.texture_set;
        let vertex_count = u32::try_from(vertex_count)
            .map_err(|_source| VulkanError::M2ParticleDrawVertexRange)?;
        first_vertex
            .checked_add(vertex_count)
            .ok_or(VulkanError::M2ParticleDrawVertexRange)?;
        let vertex_offset = i32::try_from(first_vertex)
            .map_err(|_source| VulkanError::M2ParticleDrawVertexRange)?;
        let index_count =
            u32::try_from(index_count).map_err(|_source| VulkanError::M2ParticleDrawIndexRange)?;
        first_index
            .checked_add(index_count)
            .ok_or(VulkanError::M2ParticleDrawIndexRange)?;
        Ok(M2ParticlePreparedDraw {
            pipeline,
            texture_set,
            vertex_offset,
            vertex_count,
            first_index,
            index_count,
            light_bank: M2SceneLightBank::Environment,
            scene_index: None,
            order,
            blend_order: blending_type,
            scene_order: u32::MAX,
            alpha_reference_bits: material.alpha_reference(element_alpha).to_bits(),
        })
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
    element_alpha: f32,
    order: M2EffectOrder,
    first_vertex: u32,
    first_index: u32,
    vertex_count: usize,
    index_count: usize,
) -> Result<M2ParticlePreparedDraw, VulkanError> {
    M2ParticleDrawTemplate::new(pipelines, texture_sets, pipeline, texture_set)?.prepare(
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
