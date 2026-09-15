//! Validated dynamic ribbon draw packets for unified world recording.

use solarity_asset::M2Material;

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2SceneLightBank;
use crate::device::vulkan_m2_ribbon_pipeline::{M2RibbonPipelineHandle, M2RibbonPipelineRegistry};
use crate::device::vulkan_m2_texture_set::{M2TextureSetHandle, M2TextureSetRegistry};
use crate::{M2EffectOrder, M2MaterialState};

/// Renderer-local resources and vertex range for one ribbon strip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2RibbonPreparedDraw {
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    first_vertex: u32,
    vertex_count: u32,
    light_bank: M2SceneLightBank,
    scene_index: Option<u32>,
    order: M2EffectOrder,
    blend_order: u8,
    scene_order: u32,
    first_material_pass: bool,
}

impl M2RibbonPreparedDraw {
    /// Relocates a worker-local strip and producer order into the joined frame.
    /// # Errors
    /// Returns range overflow before a relocated packet is published.
    pub fn relocate(
        mut self,
        vertices: u32,
        scene: u32,
        effects: u32,
    ) -> Result<Self, VulkanError> {
        self.first_vertex = self
            .first_vertex
            .checked_add(vertices)
            .ok_or(VulkanError::M2RibbonDrawVertexRange)?;
        self.first_vertex
            .checked_add(self.vertex_count)
            .ok_or(VulkanError::M2RibbonDrawVertexRange)?;
        self.order = M2EffectOrder::new(
            self.priority_plane(),
            self.effect_order()
                .checked_add(effects)
                .ok_or(VulkanError::M2RibbonDrawVertexRange)?,
        );
        if self.scene_order != u32::MAX {
            self.scene_order = self
                .scene_order
                .checked_add(scene)
                .ok_or(VulkanError::M2RibbonDrawVertexRange)?;
        }
        Ok(self)
    }

    /// Marks whether this pass performs the emitter's common scene setup.
    /// Later passes retain the first pass's fog registers in authored order.
    #[must_use]
    pub const fn with_first_material_pass(mut self, first: bool) -> Self {
        self.first_material_pass = first;
        self
    }

    /// Reports whether this pass publishes the emitter's common fog state.
    #[must_use]
    pub const fn first_material_pass(self) -> bool {
        self.first_material_pass
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

    /// Returns the scene-light bank selected for this ribbon instance.
    #[must_use]
    pub const fn light_bank(self) -> M2SceneLightBank {
        self.light_bank
    }

    pub(in crate::device) const fn pipeline(self) -> M2RibbonPipelineHandle {
        self.pipeline
    }

    pub(in crate::device) const fn texture_set(self) -> M2TextureSetHandle {
        self.texture_set
    }

    /// Returns the first vertex in the frame's concatenated dynamic stream.
    #[must_use]
    pub const fn first_vertex(self) -> u32 {
        self.first_vertex
    }

    /// Returns the exact even-sized triangle-strip vertex count.
    #[must_use]
    pub const fn vertex_count(self) -> u32 {
        self.vertex_count
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

    /// Assigns this ribbon's position in the unified stock scene-element queue.
    #[must_use]
    pub const fn with_scene_order(mut self, scene_order: u32) -> Self {
        self.scene_order = scene_order;
        self
    }

    /// Returns this ribbon's position in the unified scene-element queue.
    #[must_use]
    pub const fn scene_order(self) -> u32 {
        self.scene_order
    }
}

/// Immutable resource compatibility retained by a source's GPU resource lease.
#[derive(Clone, Copy)]
pub struct M2RibbonDrawTemplate {
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    material: M2MaterialState,
}

impl M2RibbonDrawTemplate {
    /// Validates resource identity once while the renderer owns its registries.
    pub(in crate::device) fn new(
        pipelines: &M2RibbonPipelineRegistry,
        textures: &M2TextureSetRegistry,
        pipeline: M2RibbonPipelineHandle,
        texture_set: M2TextureSetHandle,
    ) -> Result<Self, VulkanError> {
        let material = pipelines
            .info(pipeline)
            .ok_or(VulkanError::UnknownM2RibbonPipelineHandle)?
            .material();
        let texture = textures
            .info(texture_set)
            .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
        if texture.stage_count() != 1 {
            return Err(VulkanError::M2RibbonDrawTextureSetMismatch);
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
        material: M2Material,
        order: M2EffectOrder,
        first_vertex: u32,
        vertex_count: usize,
    ) -> Result<M2RibbonPreparedDraw, VulkanError> {
        let expected = M2MaterialState::from_material(material);
        if self.material != expected {
            return Err(VulkanError::M2RibbonDrawPipelineMismatch);
        }
        let pipeline = self.pipeline;
        let texture_set = self.texture_set;
        let vertex_count =
            u32::try_from(vertex_count).map_err(|_source| VulkanError::M2RibbonDrawVertexRange)?;
        first_vertex
            .checked_add(vertex_count)
            .ok_or(VulkanError::M2RibbonDrawVertexRange)?;
        Ok(M2RibbonPreparedDraw {
            pipeline,
            texture_set,
            first_vertex,
            vertex_count,
            light_bank: M2SceneLightBank::Environment,
            scene_index: None,
            order,
            blend_order: material.blend_mode() as u8,
            scene_order: u32::MAX,
            first_material_pass: true,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    pipelines: &M2RibbonPipelineRegistry,
    texture_sets: &M2TextureSetRegistry,
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    material: M2Material,
    order: M2EffectOrder,
    first_vertex: u32,
    vertex_count: usize,
) -> Result<M2RibbonPreparedDraw, VulkanError> {
    M2RibbonDrawTemplate::new(pipelines, texture_sets, pipeline, texture_set)?.prepare(
        material,
        order,
        first_vertex,
        vertex_count,
    )
}
