//! Complete ADT generations assembled through ordered, resumable asset stages.

use std::{collections::HashMap, ops::ControlFlow, sync::Arc};

use solarity_asset::{
    AssetStore, BlpTextureCache, BlpTextureSource, DecodedTerrainTile, M2ModelCache,
};
use solarity_rendering::TerrainTileMeshPlan;
use solarity_systems::{TerrainCollisionMesh, TerrainLiquidMesh};

use super::{
    ResidentTerrainTile, RuntimeTerrainError,
    ground_detail::{GroundDetailAssetCache, ResidentGroundDetailTile},
    m2_residency::ResidentM2SceneBuilder,
    movement::ResidentMovementReferences,
    same_doodad_placement,
    world_model_residency::{ResidentWorldModelCache, prepare_world_models},
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
    Collision(TileSurface),
    Liquid(TileSurface, TerrainCollisionMesh),
    LiquidAssets(TileSurface, TerrainCollisionMesh, TerrainLiquidMesh),
    Doodads(Box<TileInputs>, ResidentM2SceneBuilder, DoodadCursor),
    WorldModels(Box<TileInputs>, ResidentM2SceneBuilder),
}

/// One allocation retains the continuation across every texture and doodad turn.
pub(super) struct TilePreparation {
    decoded: Arc<DecodedTerrainTile>,
    specular_textures: bool,
    stage: Option<TileStage>,
}

impl TilePreparation {
    /// Decoding is complete, but none of this tile is ready for publication yet.
    pub(super) fn new(decoded: DecodedTerrainTile, specular_textures: bool) -> Box<Self> {
        Box::new(Self {
            decoded: Arc::new(decoded),
            specular_textures,
            stage: Some(TileStage::Mesh),
        })
    }

    /// Performs one complete operation; callers may drive this synchronously or
    /// return the same owned continuation to the CPU service queue. No asset
    /// request is polled or joined while holding a worker slot.
    #[allow(clippy::too_many_arguments)] // Independent private caches retain their existing owners.
    pub(super) fn advance(
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
            TileStage::GroundDetail(mesh, textures) => {
                let ground_detail = Arc::new(ground_detail_assets.prepare(
                    tile,
                    model_cache,
                    texture_cache,
                    store,
                )?);
                TileStage::Collision(TileSurface {
                    mesh,
                    textures,
                    ground_detail,
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
                if cursor.advance(tile, &mut builder, model_cache, texture_cache, store)? {
                    TileStage::WorldModels(inputs, builder)
                } else {
                    TileStage::Doodads(inputs, builder, cursor)
                }
            }
            TileStage::WorldModels(inputs, mut builder) => {
                // Stock 0x007c6150 registration follows MCRF first-reference order.
                // Keep the existing WMO routine and selected MODD traversal intact.
                let (world_models, world_model_collision, world_model_liquid) =
                    prepare_world_models(
                        tile,
                        world_model_cache,
                        model_cache,
                        texture_cache,
                        &mut builder,
                        liquid_assets,
                        store,
                    )?;
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

/// Retains MDDF table order and duplicate validation across placement boundaries.
struct DoodadCursor {
    referenced: Vec<bool>,
    next: usize,
    placements: HashMap<u32, usize>,
}

impl DoodadCursor {
    /// Strict ADT decoding has already validated every MCRF index.
    fn new(tile: &DecodedTerrainTile) -> Self {
        let mut referenced = vec![false; tile.doodads().len()];
        for reference in tile
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.doodad_references())
        {
            referenced[*reference as usize] = true;
        }
        Self {
            referenced,
            next: 0,
            placements: HashMap::new(),
        }
    }

    /// Adds at most one referenced placement; true means all MDDF entries finished.
    fn advance(
        &mut self,
        tile: &DecodedTerrainTile,
        builder: &mut ResidentM2SceneBuilder,
        models: &mut M2ModelCache,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<bool, RuntimeTerrainError> {
        while self.next < tile.doodads().len() {
            let index = self.next;
            self.next += 1;
            if !self.referenced[index] {
                continue;
            }
            let placement = &tile.doodads()[index];
            if let Some(previous) = self.placements.insert(placement.unique_id(), index) {
                if !same_doodad_placement(&tile.doodads()[previous], placement) {
                    return Err(RuntimeTerrainError::ConflictingDoodadPlacement {
                        unique_id: placement.unique_id(),
                    });
                }
            } else {
                builder.add_terrain_doodad(placement, models, textures, store)?;
            }
            return Ok(self.next == tile.doodads().len());
        }
        Ok(true)
    }
}
