//! Worker-prepared terrain residency and main-thread scene queries.

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, AssetStoreHandle, BlpTextureCache, BlpTextureSource,
    DecodedTerrainTile, M2ModelCache, MapCatalog, MapDefinition, TerrainDoodadPlacement,
    TerrainLowDetail, TerrainMap, TerrainTileIndex, WmoModelCache, WorldModelDoodadSetError,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_rendering::{
    TerrainChunkDrawPlan, TerrainLowDetailMap, TerrainTileMeshPlan, TerrainTileMeshPlanError,
    WorldCameraError, WorldFrustum,
};
use solarity_systems::{
    M2CollisionError, M2CollisionScene, PlayerCameraObstructionError,
    PlayerCameraWaterInterfaceError, TerrainCollisionError, TerrainCollisionHit,
    TerrainCollisionMesh, TerrainLiquidError, TerrainLiquidMesh, TerrainLiquidSample,
    WorldModelCollisionError, WorldModelCollisionScene, WorldModelLiquidError,
    WorldModelLiquidSample, WorldModelLiquidScene,
};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use crate::application::liquid::{
    LiquidAssetCache, ResidentTerrainLiquidBatch, RuntimeLiquidAssetError, prepare_terrain_liquids,
};

mod camera;
mod camera_profile;
mod ground_detail;
pub(in crate::application) mod m2_residency;
mod movement;
mod streaming;
pub(in crate::application) mod world_model_residency;

use ground_detail::{GroundDetailAssetCache, ResidentGroundDetailTile};
use m2_residency::{ResidentM2Scene, ResidentM2SceneBuilder};
pub(in crate::application) use movement::UnitWorldModelLocation;
pub(in crate::application) use movement::WorldModelSceneGroup;
use movement::{ResidentMovementReferences, ResidentMovementScene};
pub use movement::{
    RuntimeMovementGeometry, RuntimeMovementGeometryFailure, RuntimeMovementOwner,
    RuntimeMovementQuery, RuntimeMovementReference, RuntimeMovementRegistrationError,
    RuntimeMovementRegistrationQuery, RuntimeStaticMovementError, RuntimeStaticMovementOwner,
    RuntimeStaticMovementQuery, RuntimeStaticMovementResidency, RuntimeWorldModelMovementOwner,
};
pub use streaming::RuntimeTerrainStreamPoll;
use streaming::TerrainStreamingDemand;
use world_model_residency::{
    ResidentWorldModelScene, prepare_global_world_model, prepare_world_models,
};

/// Failure while synchronizing authored terrain with authoritative world state.
#[derive(Debug, Error)]
pub enum RuntimeTerrainError {
    /// Authored detail models could not complete on the terrain asset worker.
    #[error(transparent)]
    GroundDetail(#[from] solarity_rendering::GroundDetailError),
    /// Required liquid presentation assets could not complete on the terrain worker.
    #[error(transparent)]
    LiquidAssets(#[from] RuntimeLiquidAssetError),
    /// The bounded CPU executor rejected or lost terrain preparation work.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// Immutable M2 mesh or shader preparation failed before GPU publication.
    #[error(transparent)]
    Frame(#[from] crate::application::terrain_frame::RuntimeTerrainFrameError),
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
    /// Camera inputs could not resolve the native terrain loading window.
    #[error(transparent)]
    Streaming(#[from] solarity_systems::TerrainStreamingError),
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
    /// A hardcoded M2 texture declaration carries no archive path.
    #[error("M2 {model} has a hardcoded texture declaration without a BLP path")]
    MissingM2HardcodedTexturePath {
        /// Model whose type-zero texture declaration omitted its filename.
        model: solarity_asset::AssetPath,
    },
    /// One authored placement identity disagrees with another MODF record.
    #[error("resident terrain repeats WMO placement {unique_id} with conflicting fields")]
    ConflictingWorldModelPlacement {
        /// MODF unique identifier shared across nearby chunks and ADTs.
        unique_id: u32,
    },
    /// One authored placement identity disagrees with another MDDF record.
    #[error("resident terrain repeats M2 placement {unique_id} with conflicting fields")]
    ConflictingDoodadPlacement {
        /// MDDF unique identifier shared across nearby chunks and ADTs.
        unique_id: u32,
    },
    /// Production terrain synchronization lacks its independently mounted worker archive stack.
    #[error("terrain CPU worker archive catalog is unavailable")]
    MissingWorkerCatalog,
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
    /// Liquid triangle tracing rejected malformed geometry.
    #[error(transparent)]
    WaterSegment(#[from] solarity_systems::PlayerCameraWaterSegmentError),
    /// Camera-volume clipping rejected malformed geometry.
    #[error(transparent)]
    Volume(#[from] solarity_systems::PlayerCameraVolumeError),
    /// A camera triangle collector rejected malformed geometry.
    #[error(transparent)]
    Collection(#[from] solarity_systems::MovementCollectionError),
    /// A retained placement reference could not supply camera geometry.
    #[error(transparent)]
    StaticGeometry(#[from] RuntimeStaticMovementError),
    /// A registered WMO root could not supply camera geometry.
    #[error(transparent)]
    Registration(#[from] RuntimeMovementRegistrationError),
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
    /// The final resolved camera contains a non-finite vector.
    #[error(transparent)]
    Pose(#[from] solarity_systems::PlayerCameraPoseError),
    /// Terrain, placed-WMO, or placed-M2 obstruction resolution failed.
    #[error(transparent)]
    Obstruction(#[from] PlayerCameraObstructionError<RuntimeCameraSceneError>),
    /// Terrain or placed-WMO waterline resolution failed.
    #[error(transparent)]
    Water(#[from] PlayerCameraWaterInterfaceError<RuntimeCameraSceneError>),
    /// A camera-owned geometry query failed.
    #[error(transparent)]
    Scene(#[from] RuntimeCameraSceneError),
}

/// Observable result of one main-thread terrain synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTerrainPoll {
    /// No active world exists and no terrain is resident.
    Idle,
    /// The requested WDT/ADT generation is still being prepared off thread.
    Pending {
        /// Active client map identifier.
        map_id: u32,
    },
    /// The WDT-level WMO and its nested doodads became resident.
    GlobalWorldModelLoaded {
        /// Active client map identifier.
        map_id: u32,
    },
    /// The already-resident WDT-level WMO remains current.
    GlobalWorldModelCurrent {
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
    liquid_assets: LiquidAssetCache,
    ground_detail_assets: GroundDetailAssetCache,
    active: Option<ResidentTerrainMap>,
    worker_catalog: Option<ArchiveCatalog>,
    worker: Option<Box<TerrainWorkerState>>,
    pending: Option<PendingTerrainGeneration>,
    prefetched: Option<PrefetchedTerrainGeneration>,
    failed_request: Option<TerrainRequest>,
    streaming: Option<TerrainStreamingDemand>,
    pending_stream: Option<PendingTerrainGeneration>,
    failed_stream: std::collections::HashSet<TerrainTileIndex>,
    camera_profile: Option<camera_profile::CameraProfile>,
    camera_geometry: movement::CameraGeometry,
}

impl RuntimeTerrainCoordinator {
    /// Checks the exact Map.dbc membership before a transfer callback is admitted.
    #[must_use]
    pub fn contains_map(&self, map_id: u32) -> bool {
        self.maps.map(map_id).is_some()
    }

    /// Returns the authored map display name used in transfer-denial messages.
    #[must_use]
    pub fn map_name(&self, map_id: u32) -> Option<&str> {
        self.maps.map(map_id).map(MapDefinition::name)
    }

    pub(super) fn map_kind(&self, map_id: u32) -> Option<solarity_asset::MapKind> {
        self.maps.map(map_id).map(MapDefinition::kind)
    }

    /// Creates an empty terrain owner over the process-wide asset stack.
    #[must_use]
    pub fn new(assets: AssetStoreHandle, maps: MapCatalog) -> Self {
        Self {
            assets,
            maps,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: WmoModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
            ground_detail_assets: GroundDetailAssetCache::default(),
            active: None,
            worker_catalog: None,
            worker: None,
            pending: None,
            prefetched: None,
            failed_request: None,
            streaming: None,
            pending_stream: None,
            failed_stream: std::collections::HashSet::new(),
            camera_profile: camera_profile::CameraProfile::from_environment(),
            camera_geometry: movement::CameraGeometry::default(),
        }
    }

    /// Supplies the archive catalog used by bounded worker-side terrain preparation.
    #[must_use]
    pub fn with_worker_catalog(mut self, catalog: ArchiveCatalog) -> Self {
        self.worker_catalog = Some(catalog);
        self
    }

    /// Starts preparing the selected character's last-known terrain while the
    /// encrypted world-entry handshake is still in flight.
    ///
    /// The character-directory position is only a cache hint. The generation
    /// remains unpublished until [`Self::synchronize_async`] observes the same
    /// map and tile from authoritative world state.
    ///
    /// Returns whether a new worker task was admitted.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeTerrainError`] when the hinted map is unknown, the
    /// worker archive is unavailable, or the bounded CPU pool rejects work.
    pub fn prewarm_location(
        &mut self,
        map_id: u32,
        world_x: f32,
        world_y: f32,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimeTerrainError> {
        self.poll_stream_completion()?;
        if self.pending_stream.is_some() {
            return Ok(false);
        }
        let request = TerrainRequest::at_world_position(map_id, world_x, world_y);
        if self.active_satisfies(request)
            || self
                .prefetched
                .as_ref()
                .is_some_and(|prefetched| prefetched.request == request)
        {
            return Ok(false);
        }
        if let Some(pending) = self.pending.as_mut() {
            if pending.eligible_for_publication && pending.request == request {
                pending.retain_without_world = true;
                return Ok(false);
            }
            if !pending.task.is_finished() {
                return Ok(false);
            }
        }
        self.recover_finished_stale_worker()?;

        let definition = self
            .maps
            .map(map_id)
            .cloned()
            .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
        let source = self.take_worker_source()?;
        let task =
            cpu.try_submit(move || prepare_terrain_on_worker(source, definition, request))?;
        self.pending = Some(PendingTerrainGeneration {
            request,
            submitted_at: std::time::Instant::now(),
            retain_without_world: true,
            eligible_for_publication: true,
            task,
        });
        tracing::debug!(
            map_id,
            tile_x = request.tile.x(),
            tile_y = request.tile.y(),
            "started selected-character terrain prewarm"
        );
        Ok(true)
    }

    /// Polls complete WDT/ADT and dependency preparation on the bounded CPU pool.
    ///
    /// The currently resident generation stays available while a replacement is
    /// decoded. Only publishing the completed immutable generation into Vulkan
    /// remains on the presentation thread.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeTerrainError`] when worker admission, archive access,
    /// decoding, or dependency preparation fails.
    pub fn synchronize_async(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &CpuExecutor,
    ) -> Result<RuntimeTerrainPoll, RuntimeTerrainError> {
        let Some(world) = world else {
            self.active = None;
            self.retire_streaming();
            self.poll_stream_completion()?;
            self.failed_request = None;
            self.collect_main_thread_caches();
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.retain_without_world)
            {
                self.capture_finished_prewarm()?;
            } else {
                self.recover_finished_stale_worker()?;
            }
            return Ok(RuntimeTerrainPoll::Idle);
        };
        let map_id = world.map_id().value();
        let position = world.local_player_transform()?.position();
        let request = TerrainRequest::at_world_position(map_id, position.x, position.y);
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.map_id() != map_id)
        {
            self.retire_streaming();
        }
        self.poll_stream_completion()?;

        if let Some(prefetched) = self.prefetched.take() {
            if prefetched.request == request {
                let global_world_model = prefetched.resident.global_world_model.is_some();
                self.publish_active(prefetched.resident)?;
                self.failed_request = None;
                self.collect_main_thread_caches();
                tracing::info!(
                    map_id,
                    tile_x = request.tile.x(),
                    tile_y = request.tile.y(),
                    prewarm_age_ms = prefetched.prepared_at.elapsed().as_secs_f64() * 1_000.0,
                    "published terrain prepared during world-entry handshake"
                );
                return Ok(if global_world_model {
                    RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id }
                } else {
                    RuntimeTerrainPoll::TileLoaded {
                        map_id,
                        tile: request.tile,
                    }
                });
            }
            tracing::debug!(
                hinted_map_id = prefetched.request.map_id,
                hinted_tile_x = prefetched.request.tile.x(),
                hinted_tile_y = prefetched.request.tile.y(),
                map_id,
                tile_x = request.tile.x(),
                tile_y = request.tile.y(),
                "discarded terrain prewarm after authoritative location changed"
            );
        }

        if self.promote_resident_tile(request) {
            return Ok(RuntimeTerrainPoll::TileLoaded {
                map_id,
                tile: request.tile,
            });
        }
        if let Some(active) = self.active.as_ref()
            && self.active_satisfies(request)
        {
            if active.global_world_model.is_some() {
                self.recover_finished_stale_worker()?;
                return Ok(RuntimeTerrainPoll::GlobalWorldModelCurrent { map_id });
            }
            if active.tile.as_ref().map(ResidentTerrainTile::index) == Some(request.tile) {
                self.recover_finished_stale_worker()?;
                return Ok(RuntimeTerrainPoll::Current {
                    map_id,
                    tile: request.tile,
                });
            }
        }

        if let Some(pending) = self.pending.as_ref()
            && pending.request == request
            && !pending.task.is_finished()
        {
            return Ok(RuntimeTerrainPoll::Pending { map_id });
        }
        if let Some(pending) = self.pending.as_ref()
            && !pending.task.is_finished()
        {
            return Ok(RuntimeTerrainPoll::Pending { map_id });
        }
        if let Some(pending) = self.pending.take() {
            let pending_request = pending.request;
            let completion = pending.task.join()?;
            if let Some(worker) = completion.worker {
                self.worker = Some(worker);
            }
            if pending.eligible_for_publication && pending_request == request {
                match completion.result {
                    Ok(active) => {
                        let global_world_model = active.global_world_model.is_some();
                        self.publish_active(active)?;
                        self.failed_request = None;
                        self.collect_main_thread_caches();
                        tracing::info!(
                            map_id,
                            tile_x = request.tile.x(),
                            tile_y = request.tile.y(),
                            residency_wait_ms =
                                pending.submitted_at.elapsed().as_secs_f64() * 1_000.0,
                            "published worker-prepared terrain generation"
                        );
                        return Ok(if global_world_model {
                            RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id }
                        } else {
                            RuntimeTerrainPoll::TileLoaded {
                                map_id,
                                tile: request.tile,
                            }
                        });
                    }
                    Err(source) => {
                        self.failed_request = Some(request);
                        return Err(source);
                    }
                }
            }
        }
        if self.failed_request == Some(request) {
            return Ok(RuntimeTerrainPoll::Pending { map_id });
        }
        if self.pending_stream.is_some() {
            return Ok(RuntimeTerrainPoll::Pending { map_id });
        }

        let definition = self
            .maps
            .map(map_id)
            .cloned()
            .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
        let source = self.take_worker_source()?;
        let task =
            cpu.try_submit(move || prepare_terrain_on_worker(source, definition, request))?;
        self.pending = Some(PendingTerrainGeneration {
            request,
            submitted_at: std::time::Instant::now(),
            retain_without_world: false,
            eligible_for_publication: true,
            task,
        });
        Ok(RuntimeTerrainPoll::Pending { map_id })
    }

    fn active_satisfies(&self, request: TerrainRequest) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.map_id() == request.map_id
                && (active.global_world_model.is_some()
                    || active.tile.as_ref().map(ResidentTerrainTile::index) == Some(request.tile))
        })
    }

    fn take_worker_source(&mut self) -> Result<TerrainWorkerSource, RuntimeTerrainError> {
        if let Some(worker) = self.worker.take() {
            Ok(TerrainWorkerSource::Ready(worker))
        } else {
            Ok(TerrainWorkerSource::Catalog(
                self.worker_catalog
                    .as_ref()
                    .ok_or(RuntimeTerrainError::MissingWorkerCatalog)?
                    .clone(),
            ))
        }
    }

    fn collect_main_thread_caches(&mut self) {
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }

    fn recover_finished_stale_worker(&mut self) -> Result<(), RuntimeTerrainError> {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| !pending.task.is_finished())
        {
            return Ok(());
        }
        let pending = self
            .pending
            .take()
            .ok_or(RuntimeTerrainError::MissingWorkerCatalog)?;
        let completion = pending.task.join()?;
        if let Some(worker) = completion.worker {
            self.worker = Some(worker);
        }
        Ok(())
    }

    fn capture_finished_prewarm(&mut self) -> Result<(), RuntimeTerrainError> {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| !pending.task.is_finished())
        {
            return Ok(());
        }
        let pending = self
            .pending
            .take()
            .ok_or(RuntimeTerrainError::MissingWorkerCatalog)?;
        let completion = pending.task.join()?;
        if let Some(worker) = completion.worker {
            self.worker = Some(worker);
        }
        match completion.result {
            Ok(resident) => {
                tracing::debug!(
                    map_id = pending.request.map_id,
                    tile_x = pending.request.tile.x(),
                    tile_y = pending.request.tile.y(),
                    preparation_ms = pending.submitted_at.elapsed().as_secs_f64() * 1_000.0,
                    "selected-character terrain prewarm completed"
                );
                self.prefetched = Some(PrefetchedTerrainGeneration {
                    request: pending.request,
                    prepared_at: std::time::Instant::now(),
                    resident,
                });
            }
            Err(error) => {
                tracing::warn!(
                    map_id = pending.request.map_id,
                    tile_x = pending.request.tile.x(),
                    tile_y = pending.request.tile.y(),
                    error = %error,
                    "selected-character terrain prewarm failed; authoritative load will retry"
                );
            }
        }
        Ok(())
    }

    /// Synchronizes map and exact player-tile residency from authoritative ECS.
    ///
    /// Neighbor demand is supplied separately by `synchronize_streaming_async`
    /// after the camera owner resolves the native loading window.
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
            self.retire_streaming();
            self.textures.collect_unused();
            self.models.collect_unused();
            self.world_models.collect_unused();
            return Ok(RuntimeTerrainPoll::Idle);
        };
        let map_id = world.map_id().value();
        if self.active.as_ref().map(ResidentTerrainMap::map_id) != Some(map_id) {
            self.retire_streaming();
            let definition = self
                .maps
                .map(map_id)
                .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
            let terrain = TerrainMap::load(&mut self.assets.borrow_mut(), definition)?;
            let low_detail = load_low_detail(&mut self.assets.borrow_mut(), &terrain)?;
            self.active = Some(ResidentTerrainMap {
                terrain,
                low_detail,
                tile: None,
                global_world_model: None,
                nearby: Vec::new(),
                movement: ResidentMovementScene::default(),
            });
            // A map replacement releases its tile before collecting cache-only
            // texture sources. Shared sources remain available without reload.
            self.textures.collect_unused();
            self.models.collect_unused();
            self.world_models.collect_unused();
        }
        let position = world.local_player_transform()?.position();
        let request = TerrainRequest::at_world_position(map_id, position.x, position.y);
        if self.promote_resident_tile(request) {
            return Ok(RuntimeTerrainPoll::TileLoaded {
                map_id,
                tile: request.tile,
            });
        }
        let active = self
            .active
            .as_mut()
            .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
        if let Some(placement) = active.terrain.global_world_model() {
            active.tile = None;
            if active.global_world_model.is_some() {
                return Ok(RuntimeTerrainPoll::GlobalWorldModelCurrent { map_id });
            }
            active.global_world_model = Some(ResidentGlobalWorldModel::prepare(
                placement,
                &mut self.textures,
                &mut self.models,
                &mut self.world_models,
                &mut self.liquid_assets,
                &mut self.assets.borrow_mut(),
            )?);
            active.synchronize_movement_owners();
            self.textures.collect_unused();
            self.models.collect_unused();
            self.world_models.collect_unused();
            return Ok(RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id });
        }
        active.global_world_model = None;
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
            &mut self.ground_detail_assets,
            &mut self.liquid_assets,
            &mut self.textures,
            &mut self.models,
            &mut self.world_models,
            &mut self.assets.borrow_mut(),
        )?;
        active.validate_tile_placements(&resident)?;
        if let Some(previous) = active.tile.replace(resident)
            && self
                .streaming
                .as_ref()
                .is_some_and(|demand| demand.contains(previous.index()))
        {
            active.nearby.push(previous);
        }
        active.synchronize_movement_owners();
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
        Ok(RuntimeTerrainPoll::TileLoaded {
            map_id,
            tile: tile_index,
        })
    }

    /// Shares the map-wide horizon independently of the currently streamed ADT.
    pub fn low_detail(&self) -> Option<&Arc<TerrainLowDetailMap>> {
        self.active
            .as_ref()
            .and_then(|active| active.low_detail.as_ref())
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

    /// Resolves the active player's terrain-authored `AreaTable` identifier.
    ///
    /// Stock `0x0077FA00` selects Map.dbc field 22 for global-WMO maps and
    /// delegates tiled-map lookup to `0x007A0490` for the resident MCNK area.
    /// Either branch preserves an authored zero as absence.
    ///
    /// # Errors
    ///
    /// Returns [`WorldStateError`] when an active world has lost its required
    /// local-player transform.
    pub fn current_area_id(&self, world: &ActiveWorld) -> Result<Option<u32>, RuntimeTerrainError> {
        if let Some(map) = self.active_map()
            && map.global_world_model().is_some()
        {
            return Ok(map.global_area_id());
        }
        let position = world.local_player_transform()?.position();
        let area_id = self
            .resident_tile()
            .and_then(|tile| tile.area_id_at_world_position(position.x, position.y))
            .unwrap_or(0);
        Ok((area_id != 0).then_some(area_id))
    }

    /// Resolves a registered unit's MCNK area or the global-WMO map area.
    pub(in crate::application) fn area_id_at(&self, position: glam::Vec3) -> Option<u32> {
        let active = self.active.as_ref()?;
        if active.terrain.global_world_model().is_some() {
            return active.terrain.global_area_id();
        }
        active
            .tile_at(TerrainMap::tile_at_world_position(position.x, position.y))?
            .decoded
            .area_id_at_world_position(position.x, position.y)
            .filter(|id| *id != 0)
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
            .map(|tile| tile.mesh.as_ref())
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
        let active = self.active.as_ref()?;
        active
            .tile
            .as_ref()
            .map(|tile| &tile.world_models)
            .or_else(|| {
                active
                    .global_world_model
                    .as_ref()
                    .map(|global| &global.world_models)
            })
    }

    /// Returns the resident MDDF/MODD presentation scene for GPU publication.
    #[must_use]
    pub(super) fn resident_m2_scene(&self) -> Option<&Arc<ResidentM2Scene>> {
        let active = self.active.as_ref()?;
        active.tile.as_ref().map(|tile| &tile.m2_scene).or_else(|| {
            active
                .global_world_model
                .as_ref()
                .map(|global| &global.m2_scene)
        })
    }

    /// Returns concrete BLP sources retained by resident M2 declarations.
    #[must_use]
    pub fn resident_m2_authored_texture_count(&self) -> usize {
        self.resident_m2_scene()
            .map_or(0, |scene| scene.authored_texture_count())
    }

    /// Returns unresolved typed replacement slots retained by resident M2s.
    #[must_use]
    pub fn resident_m2_replaceable_texture_count(&self) -> usize {
        self.resident_m2_scene()
            .map_or(0, |scene| scene.replaceable_texture_count())
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

    /// Traces every resident ADT's one-sided, hole-aware collision surface.
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
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        let mut selected: Option<TerrainCollisionHit> = None;
        for tile in active.tile.iter().chain(&active.nearby) {
            let limit = selected.map_or(maximum_fraction, TerrainCollisionHit::fraction);
            if let Some(hit) = tile.collision.trace(start, end, collision_radius, limit)? {
                selected = Some(hit);
            }
        }
        Ok(selected)
    }

    /// Resolves the resident ADT point height considered at player entry.
    ///
    /// This method exposes only the terrain provider. Movement remains
    /// responsible for combining it with WMO and dynamic-world support.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainCollisionError`] when the position is not finite.
    pub fn controlled_player_terrain_height(
        &self,
        position: glam::Vec3,
    ) -> Result<Option<f32>, TerrainCollisionError> {
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        let mut selected: Option<f32> = None;
        for tile in active.tile.iter().chain(&active.nearby) {
            if let Some(height) = tile.collision.height_at(position.x, position.y)? {
                selected = Some(selected.map_or(height, |current| current.max(height)));
            }
        }
        Ok(selected)
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
        if !world_x.is_finite() || !world_y.is_finite() {
            return Err(TerrainLiquidError::NonFinitePoint);
        }
        if reference_height.is_some_and(|height| !height.is_finite()) {
            return Err(TerrainLiquidError::NonFiniteReferenceHeight);
        }
        let Some(liquid) = self
            .active
            .as_ref()
            .and_then(|active| active.tile_at(TerrainMap::tile_at_world_position(world_x, world_y)))
            .map(|tile| &tile.liquid)
        else {
            return Ok(None);
        };
        liquid.sample(world_x, world_y, reference_height)
    }

    /// Applies UnitSound_C's foot-height and water-walking admission to liquid sound flags.
    pub(super) fn unit_wet_footstep(
        &self,
        position: glam::Vec3,
        foot_height: f32,
        movement_flags: u32,
        sounds: &solarity_asset::MovementSoundCatalog,
    ) -> Result<bool, super::sound_coordinator::RuntimeSoundError> {
        if movement_flags & 0x0020_0000 != 0 {
            return Ok(false);
        }
        let terrain = self
            .sample_liquid(position.x, position.y, Some(position.z))?
            .map(|sample| (sample.height(), u32::from(sample.liquid_type())));
        let world_model = self
            .sample_world_model_liquid(position.x, position.y, Some(position.z))?
            .map(|sample| (sample.height(), sample.liquid_type()));
        let liquid = world_model.or(terrain);
        let Some((height, liquid)) = liquid else {
            return Ok(false);
        };
        let area = self
            .active
            .as_ref()
            .and_then(|active| {
                if active.terrain.global_world_model().is_some() {
                    active.terrain.global_area_id()
                } else {
                    active
                        .tile_at(TerrainMap::tile_at_world_position(position.x, position.y))
                        .and_then(|tile| {
                            tile.decoded
                                .area_id_at_world_position(position.x, position.y)
                        })
                }
            })
            .unwrap_or(0);
        let Some(flags) = sounds.liquid_flags(area, liquid) else {
            return Ok(false);
        };
        // 7A1BC0 publishes the material's bit 1 only below its admitted surface;
        // 71A030 then compares the authored bone point and excludes water-walking.
        Ok(flags & 2 != 0
            && f64::from(position.z) < f64::from(height) + f64::from(0.01_f32)
            && (flags & 4 != 0 || position.z < height)
            && f64::from(foot_height) < f64::from(height) + f64::from(0.01_f32))
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
        self.active.as_ref().map_or(0, |active| {
            active
                .tile
                .as_ref()
                .map(|tile| &tile.m2_collision)
                .or_else(|| {
                    active
                        .global_world_model
                        .as_ref()
                        .map(|global| &global.m2_collision)
                })
                .map_or(0, M2CollisionScene::instance_count)
        })
    }

    /// Returns all resident MDDF and nested MODD presentation instances.
    #[must_use]
    pub fn resident_m2_count(&self) -> usize {
        self.resident_m2_scene()
            .map_or(0, |scene| scene.placement_count())
    }

    /// Returns distinct M2/SKIN generations shared by all placed instances.
    #[must_use]
    pub fn resident_m2_source_count(&self) -> usize {
        self.resident_m2_scene()
            .map_or(0, |scene| scene.source_count())
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
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        let mut selected = None;
        for collision in active
            .tile
            .iter()
            .chain(&active.nearby)
            .map(|tile| &tile.m2_collision)
            .chain(
                active
                    .global_world_model
                    .iter()
                    .map(|global| &global.m2_collision),
            )
        {
            let candidate =
                collision.trace_camera(start, end, selected.unwrap_or(maximum_fraction))?;
            selected = nearest_fraction(selected, candidate);
        }
        Ok(selected)
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
    ) -> Result<Option<f32>, RuntimeCameraSceneError> {
        self.camera_world_model_fraction(start, end, maximum_fraction)
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
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        let mut selected: Option<WorldModelLiquidSample> = None;
        for liquid in active
            .tile
            .iter()
            .chain(&active.nearby)
            .map(|tile| &tile.world_model_liquid)
            .chain(
                active
                    .global_world_model
                    .iter()
                    .map(|global| &global.world_model_liquid),
            )
        {
            if let Some(candidate) = liquid.sample(world_x, world_y, reference_height)? {
                let preferred = selected.is_none_or(|current| {
                    reference_height.map_or(candidate.height() > current.height(), |reference| {
                        preferred_surface(
                            Some(current.height()),
                            Some(candidate.height()),
                            reference,
                        )
                        .is_some_and(|height| height != current.height())
                    })
                });
                if preferred {
                    selected = Some(candidate);
                }
            }
        }
        Ok(selected)
    }

    /// Releases map and tile residency on world disconnect.
    pub fn disconnect(&mut self) {
        self.active = None;
        self.retire_streaming();
        self.prefetched = None;
        if let Some(pending) = self.pending.as_mut() {
            pending.retain_without_world = false;
            // A same-map NEW_WORLD is a new ownership generation. Recover
            // the worker's archive handle when it finishes, but never publish
            // that retired generation just because its tile key still matches.
            pending.eligible_for_publication = false;
        }
        self.failed_request = None;
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerrainRequest {
    map_id: u32,
    tile: TerrainTileIndex,
}

impl TerrainRequest {
    fn at_world_position(map_id: u32, world_x: f32, world_y: f32) -> Self {
        Self {
            map_id,
            tile: TerrainMap::tile_at_world_position(world_x, world_y),
        }
    }
}

struct PendingTerrainGeneration {
    request: TerrainRequest,
    submitted_at: std::time::Instant,
    retain_without_world: bool,
    /// Cleared when a world replacement retires the job's ownership generation.
    eligible_for_publication: bool,
    task: CpuTask<TerrainWorkerCompletion>,
}

struct PrefetchedTerrainGeneration {
    request: TerrainRequest,
    prepared_at: std::time::Instant,
    resident: ResidentTerrainMap,
}

/// Moves one retained cache bank between jobs without enlarging catalog-only requests.
enum TerrainWorkerSource {
    Catalog(ArchiveCatalog),
    Ready(Box<TerrainWorkerState>),
}

/// Decodes native WDL data once at the terrain asset boundary.
fn load_low_detail(
    assets: &mut AssetStore,
    terrain: &TerrainMap,
) -> Result<Option<Arc<TerrainLowDetailMap>>, RuntimeTerrainError> {
    Ok(
        TerrainLowDetail::load(assets, terrain)?
            .map(|map| Arc::new(TerrainLowDetailMap::new(&map))),
    )
}

struct TerrainWorkerState {
    // One map is retained across tile jobs; None inside the pair caches absent WDLs.
    low_detail: Option<(u32, Option<Arc<TerrainLowDetailMap>>)>,
    assets: AssetStore,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: WmoModelCache,
    liquid_assets: LiquidAssetCache,
    ground_detail_assets: GroundDetailAssetCache,
}

impl TerrainWorkerState {
    fn mount(catalog: ArchiveCatalog) -> Result<Self, RuntimeTerrainError> {
        Ok(Self {
            assets: AssetStore::mount(catalog)?,
            low_detail: None,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: WmoModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
            ground_detail_assets: GroundDetailAssetCache::default(),
        })
    }

    fn prepare(
        &mut self,
        definition: &MapDefinition,
        request: TerrainRequest,
    ) -> Result<ResidentTerrainMap, RuntimeTerrainError> {
        let terrain = TerrainMap::load(&mut self.assets, definition)?;
        if self
            .low_detail
            .as_ref()
            .is_none_or(|(map_id, _)| *map_id != request.map_id)
        {
            self.low_detail = Some((request.map_id, load_low_detail(&mut self.assets, &terrain)?));
        }
        // The immutable bank is shared by the worker, active map and frame fences.
        let low_detail = self.low_detail.as_ref().and_then(|(_, map)| map.clone());
        if let Some(placement) = terrain.global_world_model() {
            let global_world_model = Some(ResidentGlobalWorldModel::prepare(
                placement,
                &mut self.textures,
                &mut self.models,
                &mut self.world_models,
                &mut self.liquid_assets,
                &mut self.assets,
            )?);
            return Ok(ResidentTerrainMap {
                terrain,
                low_detail,
                tile: None,
                global_world_model,
                nearby: Vec::new(),
                movement: ResidentMovementScene::default(),
            });
        }
        if !terrain.tile(request.tile).exists() {
            return Err(RuntimeTerrainError::MissingPlayerTile {
                map_id: request.map_id,
                tile_x: request.tile.x(),
                tile_y: request.tile.y(),
            });
        }
        let decoded = terrain.load_tile(&mut self.assets, request.tile)?;
        let tile = Some(ResidentTerrainTile::prepare(
            decoded,
            &mut self.ground_detail_assets,
            &mut self.liquid_assets,
            &mut self.textures,
            &mut self.models,
            &mut self.world_models,
            &mut self.assets,
        )?);
        Ok(ResidentTerrainMap {
            terrain,
            low_detail,
            tile,
            global_world_model: None,
            nearby: Vec::new(),
            movement: ResidentMovementScene::default(),
        })
    }

    fn collect_unused(&mut self) {
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }
}

struct TerrainWorkerCompletion {
    worker: Option<Box<TerrainWorkerState>>,
    result: Result<ResidentTerrainMap, RuntimeTerrainError>,
}

fn prepare_terrain_on_worker(
    source: TerrainWorkerSource,
    definition: MapDefinition,
    request: TerrainRequest,
) -> TerrainWorkerCompletion {
    let mut worker = match source {
        TerrainWorkerSource::Catalog(catalog) => match TerrainWorkerState::mount(catalog) {
            Ok(worker) => Box::new(worker),
            Err(source) => {
                return TerrainWorkerCompletion {
                    worker: None,
                    result: Err(source),
                };
            }
        },
        TerrainWorkerSource::Ready(worker) => worker,
    };
    let result = worker.prepare(&definition, request);
    worker.collect_unused();
    TerrainWorkerCompletion {
        worker: Some(worker),
        result,
    }
}

struct ResidentTerrainMap {
    terrain: TerrainMap,
    low_detail: Option<Arc<TerrainLowDetailMap>>,
    tile: Option<ResidentTerrainTile>,
    global_world_model: Option<ResidentGlobalWorldModel>,
    nearby: Vec<ResidentTerrainTile>,
    movement: ResidentMovementScene,
}

impl ResidentTerrainMap {
    const fn map_id(&self) -> u32 {
        self.terrain.map_id()
    }
}

/// Complete non-ADT scene owned by one WDT-level MODF placement.
struct ResidentGlobalWorldModel {
    movement_references: ResidentMovementReferences,
    m2_scene: Arc<ResidentM2Scene>,
    m2_collision: M2CollisionScene,
    world_model_collision: WorldModelCollisionScene,
    world_model_liquid: WorldModelLiquidScene,
    world_models: ResidentWorldModelScene,
}

impl ResidentGlobalWorldModel {
    /// Loads the root WMO, its selected MODD set, and every query provider.
    fn prepare(
        placement: &solarity_asset::TerrainWorldModelPlacement,
        texture_cache: &mut BlpTextureCache,
        model_cache: &mut M2ModelCache,
        world_model_cache: &mut WmoModelCache,
        liquid_assets: &mut LiquidAssetCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let mut m2_builder = ResidentM2SceneBuilder::new();
        let (world_models, world_model_collision, world_model_liquid) = prepare_global_world_model(
            placement,
            world_model_cache,
            model_cache,
            texture_cache,
            &mut m2_builder,
            liquid_assets,
            store,
        )?;
        let (m2_scene, m2_collision) = m2_builder.finish();
        let movement_references =
            ResidentMovementReferences::prepare(None, &m2_scene, &world_models);
        Ok(Self {
            movement_references,
            m2_scene: Arc::new(m2_scene),
            m2_collision,
            world_model_collision,
            world_model_liquid,
            world_models,
        })
    }
}

pub(super) struct ResidentTerrainTile {
    ground_detail: ResidentGroundDetailTile,
    movement_references: ResidentMovementReferences,
    decoded: DecodedTerrainTile,
    textures: Vec<Arc<BlpTextureSource>>,
    mesh: Arc<TerrainTileMeshPlan>,
    collision: TerrainCollisionMesh,
    liquid: TerrainLiquidMesh,
    liquid_batches: Vec<ResidentTerrainLiquidBatch>,
    // Renderer publication shares this immutable generation until its owners retire.
    m2_scene: Arc<ResidentM2Scene>,
    m2_collision: M2CollisionScene,
    world_model_collision: WorldModelCollisionScene,
    world_model_liquid: WorldModelLiquidScene,
    world_models: ResidentWorldModelScene,
}

impl ResidentTerrainTile {
    fn prepare(
        decoded: DecodedTerrainTile,
        ground_detail_assets: &mut GroundDetailAssetCache,
        liquid_assets: &mut LiquidAssetCache,
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
        let mesh = Arc::new(TerrainTileMeshPlan::prepare(&decoded)?);
        let ground_detail =
            ground_detail_assets.prepare(&decoded, model_cache, texture_cache, store)?;
        let collision = TerrainCollisionMesh::prepare(&decoded)?;
        let liquid = TerrainLiquidMesh::prepare(&decoded)?;
        let liquid_batches =
            prepare_terrain_liquids(&decoded, liquid_assets, texture_cache, store)?;
        let mut m2_builder = ResidentM2SceneBuilder::new();
        prepare_doodads(&decoded, &mut m2_builder, model_cache, texture_cache, store)?;
        let (world_models, world_model_collision, world_model_liquid) = prepare_world_models(
            &decoded,
            world_model_cache,
            model_cache,
            texture_cache,
            &mut m2_builder,
            liquid_assets,
            store,
        )?;
        let (m2_scene, m2_collision) = m2_builder.finish();
        let movement_references =
            ResidentMovementReferences::prepare(Some(&decoded), &m2_scene, &world_models);
        Ok(Self {
            movement_references,
            ground_detail,
            decoded,
            textures,
            mesh,
            collision,
            liquid,
            liquid_batches,
            m2_scene: Arc::new(m2_scene),
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
    texture_cache: &mut BlpTextureCache,
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
        builder.add_terrain_doodad(placement, cache, texture_cache, store)?;
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
