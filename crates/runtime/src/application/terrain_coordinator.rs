//! Main-thread terrain map, tile residency, and explicit frustum selection.

use solarity_asset::{
    AssetError, AssetStore, AssetStoreHandle, BlpTextureCache, BlpTextureSource,
    DecodedTerrainTile, M2ModelCache, MapCatalog, TerrainDoodadPlacement, TerrainMap,
    TerrainTileIndex, WmoModelCache, WorldModelDoodadSetError,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_rendering::{
    TerrainChunkDrawPlan, TerrainTileMeshPlan, TerrainTileMeshPlanError, WorldCameraError,
    WorldFrustum,
};
use solarity_systems::{
    M2CollisionError, M2CollisionScene, PlayerCameraObstructionError, PlayerCameraPose,
    PlayerCameraWaterError, TerrainCollisionError, TerrainCollisionHit, TerrainCollisionMesh,
    TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample, WorldModelCollisionError,
    WorldModelCollisionScene, WorldModelLiquidError, WorldModelLiquidSample, WorldModelLiquidScene,
    resolve_player_camera_obstruction, resolve_player_camera_water_collision,
};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

pub(in crate::application) mod m2_residency;
pub(in crate::application) mod world_model_residency;

use m2_residency::{ResidentM2Scene, ResidentM2SceneBuilder};
use world_model_residency::{ResidentWorldModelScene, prepare_world_models};

/// Failure while synchronizing authored terrain with authoritative world state.
#[derive(Debug, Error)]
pub enum RuntimeTerrainError {
    /// A required client asset or table failed strict decoding.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The active ECS world lost a required player invariant.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// A decoded ADT could not enter the compact renderer mesh ABI.
    #[error(transparent)]
    Mesh(#[from] TerrainTileMeshPlanError),
    /// Decoded terrain could not enter strict collision geometry.
    #[error(transparent)]
    Collision(#[from] TerrainCollisionError),
    /// Decoded MH2O data could not enter strict liquid query geometry.
    #[error(transparent)]
    Liquid(#[from] TerrainLiquidError),
    /// A referenced WMO could not enter strict placed collision geometry.
    #[error(transparent)]
    WorldModelCollision(#[from] WorldModelCollisionError),
    /// A referenced WMO could not enter strict placed liquid geometry.
    #[error(transparent)]
    WorldModelLiquid(#[from] WorldModelLiquidError),
    /// A referenced M2 could not enter strict placed collision geometry.
    #[error(transparent)]
    M2Collision(#[from] M2CollisionError),
    /// A MODF selector references no authored WMO doodad set.
    #[error(transparent)]
    WorldModelDoodadSet(#[from] WorldModelDoodadSetError),
    /// An admitted MDDF/MODD transform cannot produce an invertible matrix.
    #[error("placed M2 transform is invalid")]
    InvalidM2Placement,
    /// One authored placement identity disagrees with another MODF record.
    #[error("terrain tile repeats WMO placement {unique_id} with conflicting fields")]
    ConflictingWorldModelPlacement {
        /// MODF unique identifier shared across nearby chunks and ADTs.
        unique_id: u32,
    },
    /// One authored placement identity disagrees with another MDDF record.
    #[error("terrain tile repeats M2 placement {unique_id} with conflicting fields")]
    ConflictingDoodadPlacement {
        /// MDDF unique identifier shared across nearby chunks and ADTs.
        unique_id: u32,
    },
    /// The server selected a map absent from the mounted build's `Map.dbc`.
    #[error("active world references unknown client map {map_id}")]
    UnknownMap {
        /// Missing client map identifier.
        map_id: u32,
    },
    /// The authoritative player position resolves to an ADT omitted by WDT.
    #[error("map {map_id} omits terrain tile [{tile_x}, {tile_y}] at the player position")]
    MissingPlayerTile {
        /// Active client map identifier.
        map_id: u32,
        /// Required ADT X coordinate.
        tile_x: u8,
        /// Required ADT Y coordinate.
        tile_y: u8,
    },
}

/// Failure from one concrete resident-world camera query provider.
#[derive(Debug, Error)]
pub enum RuntimeCameraSceneError {
    /// Resident ADT collision rejected the trace.
    #[error(transparent)]
    TerrainCollision(#[from] TerrainCollisionError),
    /// Resident placed-WMO collision rejected the trace.
    #[error(transparent)]
    WorldModelCollision(#[from] WorldModelCollisionError),
    /// Resident placed-M2 collision rejected the trace.
    #[error(transparent)]
    M2Collision(#[from] M2CollisionError),
    /// Resident MH2O sampling rejected the point.
    #[error(transparent)]
    TerrainLiquid(#[from] TerrainLiquidError),
    /// Resident placed-WMO liquid sampling rejected the point.
    #[error(transparent)]
    WorldModelLiquid(#[from] WorldModelLiquidError),
}

/// Failure while composing the stock camera against the resident world scene.
#[derive(Debug, Error)]
pub enum RuntimeCameraError {
    /// Terrain, placed-WMO, or placed-M2 obstruction resolution failed.
    #[error(transparent)]
    Obstruction(#[from] PlayerCameraObstructionError<RuntimeCameraSceneError>),
    /// Terrain or placed-WMO waterline resolution failed.
    #[error(transparent)]
    Water(#[from] PlayerCameraWaterError<RuntimeCameraSceneError>),
}

/// Observable result of one main-thread terrain synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTerrainPoll {
    /// No active world exists and no terrain is resident.
    Idle,
    /// The map is a global-WMO map with no ADT tile at the player position.
    GlobalWorldModel {
        /// Active client map identifier.
        map_id: u32,
    },
    /// The exact player ADT and all 256 upload plans became resident.
    TileLoaded {
        /// Active client map identifier.
        map_id: u32,
        /// Newly resident ADT coordinate.
        tile: TerrainTileIndex,
    },
    /// The already-resident player tile remains current.
    Current {
        /// Active client map identifier.
        map_id: u32,
        /// Already-resident ADT coordinate.
        tile: TerrainTileIndex,
    },
}

/// Owns terrain residency without remounting or duplicating the archive stack.
pub struct RuntimeTerrainCoordinator {
    assets: AssetStoreHandle,
    maps: MapCatalog,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: WmoModelCache,
    active: Option<ResidentTerrainMap>,
}

impl RuntimeTerrainCoordinator {
    /// Creates an empty terrain owner over the process-wide asset stack.
    #[must_use]
    pub fn new(assets: AssetStoreHandle, maps: MapCatalog) -> Self {
        Self {
            assets,
            maps,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: WmoModelCache::new(),
            active: None,
        }
    }

    /// Synchronizes map and exact player-tile residency from authoritative ECS.
    ///
    /// This stage intentionally does not guess a preload radius. Neighbor tile
    /// admission belongs to recovered stock streaming policy; camera visibility
    /// can only filter tiles already admitted by that policy.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeTerrainError`] when the map, player transform, WDT, or
    /// exact player ADT cannot satisfy build-12340 invariants.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeTerrainPoll, RuntimeTerrainError> {
        let Some(world) = world else {
            self.active = None;
            self.textures.collect_unused();
            self.models.collect_unused();
            self.world_models.collect_unused();
            return Ok(RuntimeTerrainPoll::Idle);
        };
        let map_id = world.map_id().value();
        if self.active.as_ref().map(ResidentTerrainMap::map_id) != Some(map_id) {
            let definition = self
                .maps
                .map(map_id)
                .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
            let terrain = TerrainMap::load(&mut self.assets.borrow_mut(), definition)?;
            self.active = Some(ResidentTerrainMap {
                terrain,
                tile: None,
            });
            // A map replacement releases its tile before collecting cache-only
            // texture sources. Shared sources remain available without reload.
            self.textures.collect_unused();
            self.models.collect_unused();
            self.world_models.collect_unused();
        }
        let active = self
            .active
            .as_mut()
            .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
        if active.terrain.global_world_model().is_some() {
            active.tile = None;
            return Ok(RuntimeTerrainPoll::GlobalWorldModel { map_id });
        }
        let position = world.local_player_transform()?.position();
        let tile_index = TerrainMap::tile_at_world_position(position.x, position.y);
        if active.tile.as_ref().map(ResidentTerrainTile::index) == Some(tile_index) {
            return Ok(RuntimeTerrainPoll::Current {
                map_id,
                tile: tile_index,
            });
        }
        if !active.terrain.tile(tile_index).exists() {
            return Err(RuntimeTerrainError::MissingPlayerTile {
                map_id,
                tile_x: tile_index.x(),
                tile_y: tile_index.y(),
            });
        }
        let decoded = active
            .terrain
            .load_tile(&mut self.assets.borrow_mut(), tile_index)?;
        let resident = ResidentTerrainTile::prepare(
            decoded,
            &mut self.textures,
            &mut self.models,
            &mut self.world_models,
            &mut self.assets.borrow_mut(),
        )?;
        active.tile = Some(resident);
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
        Ok(RuntimeTerrainPoll::TileLoaded {
            map_id,
            tile: tile_index,
        })
    }

    /// Returns the active WDT manifest when a world is resident.
    #[must_use]
    pub fn active_map(&self) -> Option<&TerrainMap> {
        self.active.as_ref().map(|active| &active.terrain)
    }

    /// Returns the exact currently resident player ADT.
    #[must_use]
    pub fn resident_tile(&self) -> Option<&DecodedTerrainTile> {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.decoded)
    }

    /// Returns the resident tile's texture table in exact MTEX index order.
    ///
    /// Each source retains the archive selected by ordinary patch precedence,
    /// so an identically named HD replacement requires no alternate code path.
    #[must_use]
    pub fn resident_texture_sources(&self) -> Option<&[Arc<BlpTextureSource>]> {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| tile.textures.as_slice())
    }

    /// Returns the immutable aggregate upload plan for the resident player ADT.
    #[must_use]
    pub fn resident_mesh_plan(&self) -> Option<&TerrainTileMeshPlan> {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.mesh)
    }

    /// Returns the number of distinct root-WMO generations used by MODF.
    ///
    /// Several placements of one root share this source generation, including
    /// its decoded groups and material texture sources.
    #[must_use]
    pub fn resident_world_model_source_count(&self) -> usize {
        self.resident_world_models()
            .map_or(0, ResidentWorldModelScene::source_count)
    }

    /// Returns the resident WMO presentation scene for renderer publication.
    #[must_use]
    pub(super) fn resident_world_models(&self) -> Option<&ResidentWorldModelScene> {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.world_models)
    }

    /// Returns the resident MDDF/MODD presentation scene for GPU publication.
    #[must_use]
    pub(super) fn resident_m2_scene(&self) -> Option<&ResidentM2Scene> {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.m2_scene)
    }

    /// Returns upload plans whose bounds intersect an explicit camera frustum.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError`] if decoded chunk bounds are non-finite.
    pub fn visible_chunks(
        &self,
        frustum: WorldFrustum,
    ) -> Result<Vec<&TerrainChunkDrawPlan>, WorldCameraError> {
        let Some(tile) = self.active.as_ref().and_then(|active| active.tile.as_ref()) else {
            return Ok(Vec::new());
        };
        tile.mesh
            .chunks()
            .iter()
            .filter_map(|mesh| match mesh.is_visible(frustum) {
                Ok(true) => Some(Ok(mesh)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    /// Traces the resident ADT's one-sided, hole-aware collision surface.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainCollisionError`] when query inputs are invalid.
    pub fn trace_collision(
        &self,
        start: glam::Vec3,
        end: glam::Vec3,
        collision_radius: f32,
        maximum_fraction: f32,
    ) -> Result<Option<TerrainCollisionHit>, TerrainCollisionError> {
        let Some(collision) = self
            .active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.collision)
        else {
            return Ok(None);
        };
        collision.trace(start, end, collision_radius, maximum_fraction)
    }

    /// Samples the preferred resident MH2O surface at a world-space point.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainLiquidError`] when point or reference input is not finite.
    pub fn sample_liquid(
        &self,
        world_x: f32,
        world_y: f32,
        reference_height: Option<f32>,
    ) -> Result<Option<TerrainLiquidSample>, TerrainLiquidError> {
        let Some(liquid) = self
            .active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.liquid)
        else {
            return Ok(None);
        };
        liquid.sample(world_x, world_y, reference_height)
    }

    /// Returns the number of unique, chunk-referenced MODF placements resident.
    #[must_use]
    pub fn resident_world_model_count(&self) -> usize {
        self.resident_world_models()
            .map_or(0, ResidentWorldModelScene::placement_count)
    }

    /// Returns the number of unique, chunk-referenced MDDF placements resident.
    #[must_use]
    pub fn resident_m2_collision_count(&self) -> usize {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map_or(0, |tile| tile.m2_collision.instance_count())
    }

    /// Returns all resident MDDF and nested MODD presentation instances.
    #[must_use]
    pub fn resident_m2_count(&self) -> usize {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map_or(0, |tile| tile.m2_scene.placement_count())
    }

    /// Returns distinct M2/SKIN generations shared by all placed instances.
    #[must_use]
    pub fn resident_m2_source_count(&self) -> usize {
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map_or(0, |tile| tile.m2_scene.source_count())
    }

    /// Traces dedicated collision triangles in resident placed M2s.
    ///
    /// # Errors
    ///
    /// Returns [`M2CollisionError`] when the segment or maximum fraction is invalid.
    pub fn trace_m2_camera(
        &self,
        start: glam::Vec3,
        end: glam::Vec3,
        maximum_fraction: f32,
    ) -> Result<Option<f32>, M2CollisionError> {
        let Some(collision) = self
            .active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.m2_collision)
        else {
            return Ok(None);
        };
        collision.trace_camera(start, end, maximum_fraction)
    }

    /// Traces camera-collidable faces in the resident tile's placed WMOs.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelCollisionError`] when the segment or maximum
    /// fraction is invalid.
    pub fn trace_world_model_camera(
        &mut self,
        start: glam::Vec3,
        end: glam::Vec3,
        maximum_fraction: f32,
    ) -> Result<Option<f32>, WorldModelCollisionError> {
        let Some(collision) = self
            .active
            .as_mut()
            .and_then(|active| active.tile.as_mut())
            .map(|tile| &mut tile.world_model_collision)
        else {
            return Ok(None);
        };
        collision.trace_camera(start, end, maximum_fraction)
    }

    /// Samples the preferred resident placed-WMO liquid surface.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelLiquidError`] when point or reference input is not
    /// finite.
    pub fn sample_world_model_liquid(
        &self,
        world_x: f32,
        world_y: f32,
        reference_height: Option<f32>,
    ) -> Result<Option<WorldModelLiquidSample>, WorldModelLiquidError> {
        let Some(liquid) = self
            .active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map(|tile| &tile.world_model_liquid)
        else {
            return Ok(None);
        };
        liquid.sample(world_x, world_y, reference_height)
    }

    /// Resolves one final player camera against all resident static providers.
    ///
    /// The caller supplies the current stock CVar state explicitly. This owner
    /// contributes only admitted ADT, WMO, and MDDF-owned M2 geometry.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeCameraError`] for invalid camera policy/input or a
    /// rejected terrain/WMO/M2 query.
    pub fn resolve_player_camera(
        &mut self,
        pose: PlayerCameraPose,
        aspect_ratio: f32,
        smart_pivot: bool,
        water_collision: bool,
    ) -> Result<PlayerCameraPose, RuntimeCameraError> {
        let pose = resolve_player_camera_obstruction(
            pose,
            aspect_ratio,
            smart_pivot,
            |start, end, maximum_fraction| {
                let terrain = self
                    .trace_collision(start, end, 0.0, maximum_fraction)?
                    .map(|hit| hit.fraction());
                let world_model = self.trace_world_model_camera(start, end, maximum_fraction)?;
                let m2 = self.trace_m2_camera(start, end, maximum_fraction)?;
                Ok::<_, RuntimeCameraSceneError>(nearest_fraction(
                    nearest_fraction(terrain, world_model),
                    m2,
                ))
            },
        )?;
        Ok(resolve_player_camera_water_collision(
            pose,
            water_collision,
            smart_pivot,
            |world_x, world_y, reference_height| {
                let terrain = self
                    .sample_liquid(world_x, world_y, Some(reference_height))?
                    .map(TerrainLiquidSample::height);
                let world_model = self
                    .sample_world_model_liquid(world_x, world_y, Some(reference_height))?
                    .map(WorldModelLiquidSample::height);
                Ok::<_, RuntimeCameraSceneError>(preferred_surface(
                    terrain,
                    world_model,
                    reference_height,
                ))
            },
        )?)
    }

    /// Releases map and tile residency on world disconnect.
    pub fn disconnect(&mut self) {
        self.active = None;
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }
}

struct ResidentTerrainMap {
    terrain: TerrainMap,
    tile: Option<ResidentTerrainTile>,
}

impl ResidentTerrainMap {
    const fn map_id(&self) -> u32 {
        self.terrain.map_id()
    }
}

struct ResidentTerrainTile {
    decoded: DecodedTerrainTile,
    textures: Vec<Arc<BlpTextureSource>>,
    mesh: TerrainTileMeshPlan,
    collision: TerrainCollisionMesh,
    liquid: TerrainLiquidMesh,
    m2_scene: ResidentM2Scene,
    m2_collision: M2CollisionScene,
    world_model_collision: WorldModelCollisionScene,
    world_model_liquid: WorldModelLiquidScene,
    world_models: ResidentWorldModelScene,
}

impl ResidentTerrainTile {
    fn prepare(
        decoded: DecodedTerrainTile,
        texture_cache: &mut BlpTextureCache,
        model_cache: &mut M2ModelCache,
        world_model_cache: &mut WmoModelCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        // Resolve each MTEX entry exactly once before accepting the tile. This
        // preserves authored layer indices while avoiding partial residency.
        let textures = decoded
            .textures()
            .iter()
            .map(|path| texture_cache.load(store, path))
            .collect::<Result<Vec<_>, _>>()?;
        let mesh = TerrainTileMeshPlan::prepare(&decoded)?;
        let collision = TerrainCollisionMesh::prepare(&decoded)?;
        let liquid = TerrainLiquidMesh::prepare(&decoded)?;
        let mut m2_builder = ResidentM2SceneBuilder::new();
        prepare_doodads(&decoded, &mut m2_builder, model_cache, store)?;
        let (world_models, world_model_collision, world_model_liquid) = prepare_world_models(
            &decoded,
            world_model_cache,
            model_cache,
            texture_cache,
            &mut m2_builder,
            store,
        )?;
        let (m2_scene, m2_collision) = m2_builder.finish();
        Ok(Self {
            decoded,
            textures,
            mesh,
            collision,
            liquid,
            m2_scene,
            m2_collision,
            world_model_collision,
            world_model_liquid,
            world_models,
        })
    }

    const fn index(&self) -> TerrainTileIndex {
        self.decoded.index()
    }
}

fn prepare_doodads(
    tile: &DecodedTerrainTile,
    builder: &mut ResidentM2SceneBuilder,
    cache: &mut M2ModelCache,
    store: &mut AssetStore,
) -> Result<(), RuntimeTerrainError> {
    let mut referenced = vec![false; tile.doodads().len()];
    for reference in tile
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.doodad_references())
    {
        // Strict ADT decoding has already proven every MCRF index is in range.
        referenced[*reference as usize] = true;
    }

    let mut placements = HashMap::<u32, usize>::new();
    for (index, placement) in tile.doodads().iter().enumerate() {
        if !referenced[index] {
            continue;
        }
        if let Some(previous_index) = placements.insert(placement.unique_id(), index) {
            if !same_doodad_placement(&tile.doodads()[previous_index], placement) {
                return Err(RuntimeTerrainError::ConflictingDoodadPlacement {
                    unique_id: placement.unique_id(),
                });
            }
            continue;
        }
        builder.add_terrain_doodad(placement, cache, store)?;
    }
    Ok(())
}

fn same_doodad_placement(left: &TerrainDoodadPlacement, right: &TerrainDoodadPlacement) -> bool {
    left.path() == right.path()
        && left.position().map(f32::to_bits) == right.position().map(f32::to_bits)
        && left.rotation().map(f32::to_bits) == right.rotation().map(f32::to_bits)
        && left.scale().to_bits() == right.scale().to_bits()
        && left.flags() == right.flags()
}

fn nearest_fraction(left: Option<f32>, right: Option<f32>) -> Option<f32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn preferred_surface(left: Option<f32>, right: Option<f32>, reference: f32) -> Option<f32> {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left_above = left >= reference - 0.0001;
            let right_above = right >= reference - 0.0001;
            if left_above != right_above {
                Some(if left_above { left } else { right })
            } else if left_above {
                Some(left.min(right))
            } else {
                Some(left.max(right))
            }
        }
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
