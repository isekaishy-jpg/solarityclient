//! Validated dynamic ribbon draw packets for unified world recording.

use solarity_asset::M2Material;

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2SceneLightBank;
use crate::device::vulkan_m2_ribbon_pipeline::{M2RibbonPipelineHandle, M2RibbonPipelineRegistry};
use crate::device::vulkan_m2_texture_set::{M2TextureSetHandle, M2TextureSetRegistry};
use crate::{M2EffectOrder, M2MaterialState, M2RibbonMeshPlan};

/// Renderer-local resources and vertex range for one ribbon strip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2RibbonPreparedDraw {
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    first_vertex: u32,
    vertex_count: u32,
    light_bank: M2SceneLightBank,
    order: M2EffectOrder,
    blend_order: u8,
}

impl M2RibbonPreparedDraw {
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
    mesh: &M2RibbonMeshPlan,
) -> Result<M2RibbonPreparedDraw, VulkanError> {
    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownM2RibbonPipelineHandle)?;
    if pipeline_info.material() != M2MaterialState::from_material(material) {
        return Err(VulkanError::M2RibbonDrawPipelineMismatch);
    }
    let texture_info = texture_sets
        .info(texture_set)
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    if texture_info.stage_count() != 1 {
        return Err(VulkanError::M2RibbonDrawTextureSetMismatch);
    }
    let vertex_count = u32::try_from(mesh.vertices().len())
        .map_err(|_source| VulkanError::M2RibbonDrawVertexRange)?;
    first_vertex
        .checked_add(vertex_count)
        .ok_or(VulkanError::M2RibbonDrawVertexRange)?;
    Ok(M2RibbonPreparedDraw {
        pipeline,
        texture_set,
        first_vertex,
        vertex_count,
        light_bank: M2SceneLightBank::Environment,
        order,
        blend_order: material.blend_mode() as u8,
    })
}
