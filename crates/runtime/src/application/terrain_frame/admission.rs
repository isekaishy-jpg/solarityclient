//! Incremental GPU preparation before a neighboring ADT becomes a world owner.

use super::m2::M2SourceAdmission;
use super::{
    RuntimeTerrainFrameError, TerrainFrame, TerrainGpuTile, prepare_tile_draws,
    validate_texture_table,
};
use crate::application::terrain_coordinator::ResidentTerrainTile;
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, TerrainTileMeshPlan, VulkanRenderer,
};
use std::sync::Arc;

/// At most one tile is offered by the coordinator; its Arc pins exact resource identity.
pub(super) struct TerrainTileAdmission {
    pub(super) plan: Arc<TerrainTileMeshPlan>,
    phase: TileAdmissionPhase,
    pub(super) tile: Option<TerrainGpuTile>,
    m2_source: Option<M2SourceAdmission>,
}

/// Each cold resource operation gets a separate presentation service.
enum TileAdmissionPhase {
    Geometry,
    Material,
    Textures(usize),
    Draws,
    Liquids,
    Models(usize),
    WorldModels(usize),
    Ready,
}

impl TerrainFrame {
    /// Cancels staged resources without disturbing any visible placement or random clock.
    pub(super) fn clear_tile_admission(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if let Some(admission) = self.tile_admission.take() {
            if let Some(tile) = admission.tile {
                let handles = tile
                    .liquids
                    .iter()
                    .map(|batch| batch.mesh())
                    .collect::<Vec<_>>();
                renderer.retire_liquid_meshes(&handles)?;
            }
            renderer.retire_terrain_plans(std::iter::once(admission.plan.as_ref()))?;
        }
        self.m2.clear_prepared_static();
        self.world_models.clear_prepared_static(renderer)?;
        Ok(())
    }

    /// Returns readiness only after all upload resources exist; the caller then
    /// commits CPU collision and GPU membership in the same service transaction.
    pub(in crate::application) fn admit_tile(
        &mut self,
        renderer: &mut VulkanRenderer,
        resident: &ResidentTerrainTile,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if self
            .tile_admission
            .as_ref()
            .is_none_or(|admission| !Arc::ptr_eq(&admission.plan, resident.mesh()))
        {
            self.clear_tile_admission(renderer)?;
            validate_texture_table(resident.mesh(), resident.textures())?;
            self.tile_admission = Some(TerrainTileAdmission {
                plan: Arc::clone(resident.mesh()),
                phase: TileAdmissionPhase::Geometry,
                tile: None,
                m2_source: None,
            });
        }
        let Some(admission) = self.tile_admission.as_mut() else {
            return Ok(false);
        };
        loop {
            match admission.phase {
                TileAdmissionPhase::Geometry => {
                    renderer.upload_terrain_mesh(resident.mesh())?;
                    admission.phase = TileAdmissionPhase::Material;
                }
                TileAdmissionPhase::Material => {
                    renderer.upload_terrain_material(resident.mesh())?;
                    admission.phase = TileAdmissionPhase::Textures(0);
                }
                TileAdmissionPhase::Textures(index) => {
                    let Some(source) = resident.textures().get(index) else {
                        admission.phase = TileAdmissionPhase::Draws;
                        continue;
                    };
                    renderer.upload_blp_textures(&[BlpTextureUploadRequest::new(
                        source,
                        BlpColorSpace::Linear,
                    )])?;
                    admission.phase = TileAdmissionPhase::Textures(index + 1);
                }
                TileAdmissionPhase::Draws => {
                    admission.tile = Some(TerrainGpuTile {
                        plan: Arc::clone(resident.mesh()),
                        draws: prepare_tile_draws(renderer, resident.mesh(), resident.textures())?,
                        liquids: Vec::new(),
                    });
                    admission.phase = TileAdmissionPhase::Liquids;
                }
                TileAdmissionPhase::Liquids => {
                    let liquids = self
                        .liquid_materials
                        .prepare_terrain(renderer, resident.liquid_batches())?;
                    if let Some(tile) = admission.tile.as_mut() {
                        tile.liquids = liquids;
                    }
                    admission.phase = TileAdmissionPhase::Models(0);
                }
                TileAdmissionPhase::Models(index) => {
                    let Some(source) = resident.m2_scene().sources().get(index) else {
                        admission.phase = TileAdmissionPhase::WorldModels(0);
                        continue;
                    };
                    if self.m2.static_source_is_prepared(source) {
                        admission.phase = TileAdmissionPhase::Models(index + 1);
                        continue;
                    }
                    if self
                        .m2
                        .prepare_static_source(renderer, source, &mut admission.m2_source)?
                    {
                        admission.phase = TileAdmissionPhase::Models(index + 1);
                    }
                }
                TileAdmissionPhase::WorldModels(index) => {
                    let Some(source) = resident.world_models().sources().get(index) else {
                        admission.phase = TileAdmissionPhase::Ready;
                        continue;
                    };
                    if self.world_models.static_source_is_prepared(source) {
                        admission.phase = TileAdmissionPhase::WorldModels(index + 1);
                        continue;
                    }
                    self.world_models.prepare_static_source(renderer, source)?;
                    admission.phase = TileAdmissionPhase::WorldModels(index + 1);
                }
                TileAdmissionPhase::Ready => return Ok(true),
            }
            return Ok(false);
        }
    }
}
