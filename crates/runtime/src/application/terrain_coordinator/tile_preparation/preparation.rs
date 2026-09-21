//! Ordered tile material, query and placed-source preparation.

use super::{SharedTerrainSources, doodads::DoodadCursor};
use std::{ops::ControlFlow, sync::Arc};

use solarity_asset::{
    AssetStore, BlpTextureCache, BlpTextureSource, DecodedTerrainTile, M2ModelCache,
};
use solarity_rendering::TerrainTileMeshPlan;
use solarity_systems::{TerrainCollisionMesh, TerrainLiquidMesh};

use super::super::{
    ResidentTerrainTile, RuntimeTerrainError,
    ground_detail::{GroundDetailAssetCache, GroundDetailPreparation, ResidentGroundDetailTile},
    m2_residency::ResidentM2SceneBuilder,
    movement::ResidentMovementReferences,
    world_model_residency::{ResidentWorldModelCache, WorldModelPreparation},
};
use crate::application::liquid::{
    LiquidAssetCache, ResidentTerrainLiquidBatch, prepare_terrain_liquids,
};

/// Surface inputs exist together only after authored textures and ground detail succeed.
struct TileSurface {
    mesh: Arc<TerrainTileMeshPlan>,
    textures: Vec<Arc<BlpTextureSource>>,
    ground_detail: Arc<ResidentGroundDetailTile>,
}

/// Query and surface data remain private until all placed resources also succeed.
struct TileInputs {
    surface: TileSurface,
    collision: TerrainCollisionMesh,
    liquid: TerrainLiquidMesh,
    liquid_batches: Vec<ResidentTerrainLiquidBatch>,
}

/// Variants preserve first-error order without optional partially initialized fields.
enum TileStage {
    Mesh,
    Textures(Arc<TerrainTileMeshPlan>, Vec<Arc<BlpTextureSource>>),
    GroundDetail(Arc<TerrainTileMeshPlan>, Vec<Arc<BlpTextureSource>>),
    GroundDetailSources(
        Arc<TerrainTileMeshPlan>,
        Vec<Arc<BlpTextureSource>>,
        GroundDetailPreparation,
    ),
    Collision(TileSurface),
    Liquid(TileSurface, TerrainCollisionMesh),
    LiquidAssets(TileSurface, TerrainCollisionMesh, TerrainLiquidMesh),
    Doodads(Box<TileInputs>, ResidentM2SceneBuilder, DoodadCursor),
    WorldModels(
        Box<TileInputs>,
        ResidentM2SceneBuilder,
        Box<WorldModelPreparation>,
    ),
}

/// One allocation retains the continuation across every texture and doodad turn.
pub(in super::super) struct TilePreparation {
    decoded: Arc<DecodedTerrainTile>,
    specular_textures: bool,
    stage: Option<TileStage>,
    shared: Option<SharedTerrainSources>,
    suspension: Option<solarity_cpu::CpuTaskDependency>,
}

impl TilePreparation {
    /// A cancelled tile releases consumers immediately and drains only an owned
    /// shared WMO producer. Partial terrain remains private throughout retirement.
    pub(in super::super) fn retire_source_step(&mut self, store: &mut AssetStore) -> bool {
        match self.stage.as_mut() {
            Some(TileStage::WorldModels(_, _, models)) => models.retire_source_step(store),
            _ => true,
        }
    }

    /// Decoding is complete, but none of this tile is ready for publication yet.
    pub(in super::super) fn new(decoded: DecodedTerrainTile, specular_textures: bool) -> Box<Self> {
        Box::new(Self {
            decoded: Arc::new(decoded),
            specular_textures,
            stage: Some(TileStage::Mesh),
            shared: None,
            suspension: None,
        })
    }

    /// Runtime loading joins namespace-owned sources; synchronous tools keep
    /// their explicit local cache execution contract through `new`.
    pub(in super::super) fn with_shared_sources(
        decoded: DecodedTerrainTile,
        specular_textures: bool,
        shared: SharedTerrainSources,
    ) -> Box<Self> {
        let mut pending = Self::new(decoded, specular_textures);
        pending.shared = Some(shared);
        pending
    }

    /// The owned continuation keeps the typed source lease; only readiness moves.
    pub(in super::super) fn take_dependency(&mut self) -> Option<solarity_cpu::CpuTaskDependency> {
        self.suspension.take()
    }

    /// Performs one complete operation; callers may drive this synchronously or
    /// return the same owned continuation to the CPU service queue. No asset
    /// request is polled or joined while holding a worker slot.
    #[allow(clippy::too_many_arguments)] // Independent private caches retain their existing owners.
    pub(in super::super) fn advance(
        mut self: Box<Self>,
        ground_detail_assets: &mut GroundDetailAssetCache,
        liquid_assets: &mut LiquidAssetCache,
        texture_cache: &mut BlpTextureCache,
        model_cache: &mut M2ModelCache,
        world_model_cache: &mut ResidentWorldModelCache,
        store: &mut AssetStore,
    ) -> Result<ControlFlow<ResidentTerrainTile, Box<Self>>, RuntimeTerrainError> {
        let tile = &self.decoded;
        let stage = self
            .stage
            .take()
            .unwrap_or_else(|| unreachable!("only unfinished tiles can advance"));
        let next = match stage {
            TileStage::Mesh => {
                let mesh = Arc::new(TerrainTileMeshPlan::prepare_with_specular(
                    tile,
                    self.specular_textures,
                )?);
                let textures = Vec::with_capacity(mesh.textures().len());
                TileStage::Textures(mesh, textures)
            }
            TileStage::Textures(mesh, mut textures) => {
                // MTEX indices and error precedence are authored table order.
                if let Some(path) = mesh.textures().get(textures.len()) {
                    textures.push(texture_cache.load(store, path)?);
                }
                if textures.len() == mesh.textures().len() {
                    TileStage::GroundDetail(mesh, textures)
                } else {
                    TileStage::Textures(mesh, textures)
                }
            }
            TileStage::GroundDetail(mesh, textures) => TileStage::GroundDetailSources(
                mesh,
                textures,
                ground_detail_assets.begin(tile, store)?,
            ),
            TileStage::GroundDetailSources(mesh, textures, mut preparation) => {
                if !preparation.advance(
                    ground_detail_assets,
                    model_cache,
                    texture_cache,
                    store,
                    self.shared.as_ref(),
                    &mut self.suspension,
                )? {
                    self.stage = Some(TileStage::GroundDetailSources(mesh, textures, preparation));
                    return Ok(ControlFlow::Continue(self));
                }
                TileStage::Collision(TileSurface {
                    mesh,
                    textures,
                    ground_detail: Arc::new(preparation.finish()),
                })
            }
            TileStage::Collision(surface) => {
                TileStage::Liquid(surface, TerrainCollisionMesh::prepare(tile)?)
            }
            TileStage::Liquid(surface, collision) => {
                TileStage::LiquidAssets(surface, collision, TerrainLiquidMesh::prepare(tile)?)
            }
            TileStage::LiquidAssets(surface, collision, liquid) => {
                let liquid_batches =
                    prepare_terrain_liquids(tile, liquid_assets, texture_cache, store)?;
                TileStage::Doodads(
                    Box::new(TileInputs {
                        surface,
                        collision,
                        liquid,
                        liquid_batches,
                    }),
                    ResidentM2SceneBuilder::new(),
                    DoodadCursor::new(tile),
                )
            }
            TileStage::Doodads(inputs, mut builder, mut cursor) => {
                let finished = cursor.advance(
                    tile,
                    &mut builder,
                    model_cache,
                    texture_cache,
                    store,
                    self.shared.as_ref(),
                    &mut self.suspension,
                )?;
                if finished {
                    TileStage::WorldModels(inputs, builder, WorldModelPreparation::for_tile(tile))
                } else {
                    TileStage::Doodads(inputs, builder, cursor)
                }
            }
            TileStage::WorldModels(inputs, mut builder, mut pending) => {
                // Stock 0x007c6150 registration follows MCRF first-reference order.
                // Keep the existing WMO routine and selected MODD traversal intact.
                if !pending.advance(
                    world_model_cache,
                    model_cache,
                    texture_cache,
                    &mut builder,
                    liquid_assets,
                    store,
                    self.shared.as_ref(),
                    &mut self.suspension,
                )? {
                    self.stage = Some(TileStage::WorldModels(inputs, builder, pending));
                    return Ok(ControlFlow::Continue(self));
                }
                let (world_models, world_model_collision, world_model_liquid) = pending.finish();
                let (m2_scene, m2_collision) = builder.finish();
                let movement_references =
                    ResidentMovementReferences::prepare(Some(tile), &m2_scene, &world_models);
                let TileInputs {
                    surface,
                    collision,
                    liquid,
                    liquid_batches,
                } = *inputs;
                return Ok(ControlFlow::Break(ResidentTerrainTile {
                    ground_detail: surface.ground_detail,
                    textures: surface.textures,
                    mesh: surface.mesh,
                    decoded: self.decoded,
                    collision,
                    liquid,
                    liquid_batches,
                    movement_references,
                    m2_scene: Arc::new(m2_scene),
                    m2_collision,
                    world_models,
                    world_model_collision,
                    world_model_liquid,
                }));
            }
        };
        self.stage = Some(next);
        Ok(ControlFlow::Continue(self))
    }
}
