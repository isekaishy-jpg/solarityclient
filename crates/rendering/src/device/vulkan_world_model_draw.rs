//! Validated join of one logical MOBA pass to renderer-local GPU resources.

use glam::{Mat4, Vec3};

use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::vulkan_world_model_mesh::{WorldModelMeshHandle, WorldModelMeshRegistry};
use crate::device::vulkan_world_model_pipeline::{
    WorldModelPipelineHandle, WorldModelPipelineRegistry,
};
use crate::device::vulkan_world_model_texture_set::{
    WorldModelTextureSetHandle, WorldModelTextureSetRegistry,
};
use crate::device::{BlpColorSpace, BlpTextureSourceKind, VulkanError};
use crate::{
    WorldModelMaterialUniform, WorldModelMeshPlan, WorldModelSpirvKey, WorldModelSurfacePassPlan,
};

/// One immutable physical WMO pass ready for future world-frame recording.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelPreparedDraw {
    submission_fog_color: Option<Vec3>,
    submission_fog_outdoor: bool,
    mesh: WorldModelMeshHandle,
    pipeline: WorldModelPipelineHandle,
    texture_set: WorldModelTextureSetHandle,
    first_index: u32,
    index_count: u32,
    material: WorldModelMaterialUniform,
}

/// A validated WMO silhouette range using shared geometry and its first texture.
/// It does not require an ordinary surface pipeline or a visible callback batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelShadowDraw {
    mesh: WorldModelMeshHandle,
    texture_set: WorldModelTextureSetHandle,
    first_index: u32,
    index_count: u32,
    material: WorldModelMaterialUniform,
}

impl WorldModelShadowDraw {
    /// Returns the shared geometry identity.
    pub const fn mesh(self) -> WorldModelMeshHandle {
        self.mesh
    }
    /// Returns the descriptor whose first texture supplies cutout coverage.
    pub const fn texture_set(self) -> WorldModelTextureSetHandle {
        self.texture_set
    }
    /// Returns the first combined index and submitted count.
    pub const fn index_range(self) -> [u32; 2] {
        [self.first_index, self.index_count]
    }
    /// Returns the model transform in the existing world material ABI.
    pub const fn material(self) -> WorldModelMaterialUniform {
        self.material
    }
}

impl From<WorldModelPreparedDraw> for WorldModelShadowDraw {
    fn from(draw: WorldModelPreparedDraw) -> Self {
        Self {
            mesh: draw.mesh,
            texture_set: draw.texture_set,
            first_index: draw.first_index,
            index_count: draw.index_count,
            material: draw.material,
        }
    }
}

/// Validates a complete MOBA-table draw without preparing an unused surface pass.
#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_shadow_draw(
    meshes: &WorldModelMeshRegistry,
    texture_sets: &WorldModelTextureSetRegistry,
    textures: &BlpTextureRegistry,
    mesh: WorldModelMeshHandle,
    texture_set: WorldModelTextureSetHandle,
    plan: &WorldModelMeshPlan,
    draw_index: usize,
    model: Mat4,
) -> Result<WorldModelShadowDraw, VulkanError> {
    let mesh_info = meshes
        .info(mesh)
        .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
    if mesh_info.path() != plan.path()
        || mesh_info.vertex_count() != plan.vertices().len()
        || mesh_info.index_count() != plan.indices().len()
    {
        return Err(VulkanError::WorldModelDrawMeshMismatch);
    }
    let draw =
        plan.shadow_draws()
            .get(draw_index)
            .copied()
            .ok_or(VulkanError::WorldModelDrawIndex {
                requested: draw_index,
                available: plan.shadow_draws().len(),
            })?;
    let end = draw
        .first_index()
        .checked_add(draw.index_count())
        .ok_or(VulkanError::WorldModelDrawIndexRange)?;
    if end as usize > mesh_info.index_count() {
        return Err(VulkanError::WorldModelDrawIndexRange);
    }
    let material = plan
        .materials()
        .get(usize::from(draw.material_id()))
        .ok_or(VulkanError::WorldModelDrawMaterial)?;
    let request = texture_sets
        .request(texture_set)
        .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
    let first = request
        .stages()
        .first()
        .ok_or(VulkanError::WorldModelDrawTextureSetMismatch)?;
    let texture = textures
        .info(first.texture())
        .ok_or(VulkanError::UnknownWorldModelTextureHandle)?;
    let matches = material.textures()[0].as_ref().map_or_else(
        || texture.source_kind() == BlpTextureSourceKind::StockWorldModelGreen,
        |path| texture.source_kind() == BlpTextureSourceKind::Authored && texture.path() == path,
    );
    if !matches || texture.color_space() != BlpColorSpace::Linear {
        return Err(VulkanError::WorldModelDrawTextureSetMismatch);
    }
    Ok(WorldModelShadowDraw {
        mesh,
        texture_set,
        first_index: draw.first_index(),
        index_count: draw.index_count(),
        material: WorldModelMaterialUniform::shadow(model),
    })
}

impl WorldModelPreparedDraw {
    /// Supplies the ordinary bank for native WMO paths which force exterior fog.
    /// Updates both surface shading and the bank retained for following effects.
    #[must_use]
    pub fn with_outdoor_fog_color(mut self, color: Vec3) -> Self {
        if self.submission_fog_outdoor {
            self.material = self.material.with_fog_color(color);
            if self.submission_fog_color.is_some() {
                self.submission_fog_color = Some(color);
            }
        }
        self
    }

    pub(in crate::device) const fn submission_fog_color(self) -> Option<Vec3> {
        self.submission_fog_color
    }

    /// Returns the renderer-local combined geometry identity.
    #[must_use]
    pub const fn mesh(self) -> WorldModelMeshHandle {
        self.mesh
    }

    /// Returns the renderer-local physical-pass pipeline identity.
    #[must_use]
    pub const fn pipeline(self) -> WorldModelPipelineHandle {
        self.pipeline
    }

    /// Returns the renderer-local root-material descriptor identity.
    #[must_use]
    pub const fn texture_set(self) -> WorldModelTextureSetHandle {
        self.texture_set
    }

    /// Returns the first combined `u32` index and submitted count.
    #[must_use]
    pub const fn index_range(self) -> [u32; 2] {
        [self.first_index, self.index_count]
    }

    /// Returns the exact std140 material payload for this pass and placement.
    #[must_use]
    pub const fn material(self) -> WorldModelMaterialUniform {
        self.material
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::device) fn prepare_draw(
    meshes: &WorldModelMeshRegistry,
    pipelines: &WorldModelPipelineRegistry,
    texture_sets: &WorldModelTextureSetRegistry,
    textures: &BlpTextureRegistry,
    mesh: WorldModelMeshHandle,
    pipeline: WorldModelPipelineHandle,
    texture_set: WorldModelTextureSetHandle,
    plan: &WorldModelMeshPlan,
    draw_index: usize,
    pass_index: usize,
    model: Mat4,
    environment_emissive: f32,
    fog_color: Vec3,
) -> Result<WorldModelPreparedDraw, VulkanError> {
    let mesh_info = meshes
        .info(mesh)
        .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
    if mesh_info.path() != plan.path()
        || mesh_info.vertex_count() != plan.vertices().len()
        || mesh_info.index_count() != plan.indices().len()
    {
        return Err(VulkanError::WorldModelDrawMeshMismatch);
    }
    let draw = plan
        .draws()
        .get(draw_index)
        .copied()
        .ok_or(VulkanError::WorldModelDrawIndex {
            requested: draw_index,
            available: plan.draws().len(),
        })?;
    let index_end = draw
        .first_index()
        .checked_add(draw.index_count())
        .ok_or(VulkanError::WorldModelDrawIndexRange)?;
    let uploaded_count = u32::try_from(mesh_info.index_count())
        .map_err(|_source| VulkanError::WorldModelDrawIndexRange)?;
    if index_end > uploaded_count {
        return Err(VulkanError::WorldModelDrawIndexRange);
    }
    let material = plan
        .materials()
        .get(usize::from(draw.material_id()))
        .ok_or(VulkanError::WorldModelDrawMaterial)?;
    let group_flags = plan
        .groups()
        .iter()
        .find(|group| group.group_index() == draw.group_index())
        .map(|group| group.flags())
        .ok_or(VulkanError::WorldModelDrawGroup)?;
    let passes =
        WorldModelSurfacePassPlan::prepare(plan.root_flags(), group_flags, draw.class(), material);
    let pass = passes
        .passes()
        .get(pass_index)
        .copied()
        .ok_or(VulkanError::WorldModelDrawPass {
            requested: pass_index,
            available: passes.passes().len(),
        })?;
    let pipeline_info = pipelines
        .info(pipeline)
        .ok_or(VulkanError::UnknownWorldModelPipelineHandle)?;
    let expected_effect = WorldModelSpirvKey::new(pass.material().shader(), passes.is_unified())
        .map_err(|error| VulkanError::WorldModelShader {
            message: error.to_string(),
        })?;
    if pipeline_info.effect() != expected_effect || pipeline_info.material() != pass.material() {
        return Err(VulkanError::WorldModelDrawPipelineMismatch);
    }
    let texture_request = texture_sets
        .request(texture_set)
        .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
    if texture_request.stage_count() != pass.material().texture_count() {
        return Err(VulkanError::WorldModelDrawTextureSetMismatch);
    }
    for (slot, stage) in texture_request.stages().iter().copied().enumerate() {
        let actual = textures
            .info(stage.texture())
            .ok_or(VulkanError::UnknownWorldModelTextureHandle)?;
        let source_matches = material.textures()[slot].as_ref().map_or_else(
            || actual.source_kind() == BlpTextureSourceKind::StockWorldModelGreen,
            |expected_path| {
                actual.source_kind() == BlpTextureSourceKind::Authored
                    && actual.path() == expected_path
            },
        );
        if !source_matches || actual.color_space() != BlpColorSpace::Linear {
            return Err(VulkanError::WorldModelDrawTextureSetMismatch);
        }
    }
    // 7AC6A0 uses the ordinary bank. 7AC9F0's final pass uses the selected
    // group bank. 7A9380 also forces fog for nontransition batches, selecting
    // the ordinary bank for exterior-lit groups regardless of MOMT unfogged.
    let submission_fog_color = (pass_index + 1 == passes.passes().len()
        && pass.fog_mode() != crate::WorldModelFogMode::Disabled)
        .then_some(fog_color);
    let submission_fog_outdoor = pass.fog_mode() == crate::WorldModelFogMode::OutdoorColor;
    let material = WorldModelMaterialUniform::new(
        model,
        plan.ambient_color(),
        material,
        pass,
        environment_emissive,
        fog_color,
    );
    Ok(WorldModelPreparedDraw {
        submission_fog_color,
        submission_fog_outdoor,
        mesh,
        pipeline,
        texture_set,
        first_index: draw.first_index(),
        index_count: draw.index_count(),
        material,
    })
}
