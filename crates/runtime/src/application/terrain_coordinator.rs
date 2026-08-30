//! Main-thread terrain map, tile residency, and explicit frustum selection.

use solarity_asset::{
    AssetError, AssetStore, AssetStoreHandle, BlpTextureCache, BlpTextureSource,
    DecodedTerrainTile, MapCatalog, TerrainMap, TerrainTileIndex, TerrainWorldModelPlacement,
    WmoModelCache,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_rendering::{
    TerrainChunkDrawPlan, TerrainTileMeshPlan, TerrainTileMeshPlanError, WorldCameraError,
    WorldFrustum,
};
use solarity_systems::{
    PlacedWorldModelCollision, PlacedWorldModelLiquid, TerrainCollisionError, TerrainCollisionHit,
    TerrainCollisionMesh, TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample,
    WorldModelCollisionError, WorldModelCollisionScene, WorldModelLiquidError,
    WorldModelLiquidSample, WorldModelLiquidScene,
};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

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
    /// One authored placement identity disagrees with another MODF record.
    #[error("terrain tile repeats WMO placement {unique_id} with conflicting fields")]
    ConflictingWorldModelPlacement {
        /// MODF unique identifier shared across nearby chunks and ADTs.
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
            &mut self.world_models,
            &mut self.assets.borrow_mut(),
        )?;
        active.tile = Some(resident);
        self.textures.collect_unused();
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
        self.active
            .as_ref()
            .and_then(|active| active.tile.as_ref())
            .map_or(0, |tile| tile.world_model_collision.instance_count())
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

    /// Releases map and tile residency on world disconnect.
    pub fn disconnect(&mut self) {
        self.active = None;
        self.textures.collect_unused();
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
    world_model_collision: WorldModelCollisionScene,
    world_model_liquid: WorldModelLiquidScene,
}

impl ResidentTerrainTile {
    fn prepare(
        decoded: DecodedTerrainTile,
        texture_cache: &mut BlpTextureCache,
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
        let (world_model_collision, world_model_liquid) =
            prepare_world_models(&decoded, world_model_cache, store)?;
        Ok(Self {
            decoded,
            textures,
            mesh,
            collision,
            liquid,
            world_model_collision,
            world_model_liquid,
        })
    }

    const fn index(&self) -> TerrainTileIndex {
        self.decoded.index()
    }
}

fn prepare_world_models(
    tile: &DecodedTerrainTile,
    cache: &mut WmoModelCache,
    store: &mut AssetStore,
) -> Result<(WorldModelCollisionScene, WorldModelLiquidScene), RuntimeTerrainError> {
    let mut referenced = vec![false; tile.world_models().len()];
    for reference in tile
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.world_model_references())
    {
        // Strict ADT decoding has already proven every MCRF index is in range.
        referenced[*reference as usize] = true;
    }

    let mut scene = WorldModelCollisionScene::new();
    let mut liquids = WorldModelLiquidScene::new();
    let mut placements = HashMap::<u32, usize>::new();
    for (index, placement) in tile.world_models().iter().enumerate() {
        if !referenced[index] {
            continue;
        }
        if let Some(previous_index) = placements.insert(placement.unique_id(), index) {
            if !same_world_model_placement(&tile.world_models()[previous_index], placement) {
                return Err(RuntimeTerrainError::ConflictingWorldModelPlacement {
                    unique_id: placement.unique_id(),
                });
            }
            continue;
        }
        let model = cache.load(store, placement.path())?;
        scene.add(PlacedWorldModelCollision::prepare(
            Arc::clone(&model),
            glam::Vec3::from_array(placement.position()),
            glam::Vec3::from_array(placement.rotation()),
            1.0,
        )?);
        liquids.add(PlacedWorldModelLiquid::prepare(
            model,
            glam::Vec3::from_array(placement.position()),
            glam::Vec3::from_array(placement.rotation()),
            1.0,
        )?);
    }
    Ok((scene, liquids))
}

fn same_world_model_placement(
    left: &TerrainWorldModelPlacement,
    right: &TerrainWorldModelPlacement,
) -> bool {
    left.path() == right.path()
        && left.position().map(f32::to_bits) == right.position().map(f32::to_bits)
        && left.rotation().map(f32::to_bits) == right.rotation().map(f32::to_bits)
        && left.bounds().map(|point| point.map(f32::to_bits))
            == right.bounds().map(|point| point.map(f32::to_bits))
        && left.flags() == right.flags()
        && left.doodad_set() == right.doodad_set()
        && left.name_set() == right.name_set()
}
