//! Main-thread ownership of worker-prepared neighboring ADT generations.

use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3;
use solarity_asset::{DecodedTerrainTile, TerrainTileIndex};
use solarity_cpu::CpuExecutor;
use solarity_systems::{TerrainStreamingWindow, prioritize_terrain_tiles};

use super::{
    PendingTerrainGeneration, ResidentTerrainMap, ResidentTerrainTile, RuntimeTerrainCoordinator,
    RuntimeTerrainError, TerrainRequest, prepare_terrain_on_worker,
};

/// Whether every declared ADT in the current outer loading window is resident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTerrainStreamPoll {
    /// No matching tiled world has been admitted yet.
    Idle,
    /// One or more declared ADTs still require complete asset preparation.
    Pending {
        /// Number of declared ADTs whose complete generation is unavailable.
        remaining_tiles: usize,
    },
    /// Every declared tile and its static dependencies are resident.
    Current,
}

/// Latest camera demand and retained native request priority.
pub(super) struct TerrainStreamingDemand {
    origin: Vec3,
    window: TerrainStreamingWindow,
    /// Native ADT registrations persist even before their asset load completes.
    owners: Vec<TerrainTileIndex>,
    tiles: Vec<TerrainTileIndex>,
}

impl TerrainStreamingDemand {
    /// Whether a completed owner still belongs to this camera's retained range.
    pub(super) fn contains(&self, tile: TerrainTileIndex) -> bool {
        self.window.contains(tile)
    }
}

impl RuntimeTerrainCoordinator {
    /// Borrows the complete primary tile followed by retained neighboring generations.
    pub(in crate::application) fn resident_tiles(
        &self,
    ) -> impl Iterator<Item = &ResidentTerrainTile> + Clone {
        self.active
            .iter()
            .flat_map(|active| active.tile.iter().chain(&active.nearby))
    }

    /// Advances the native camera window without blocking the main thread.
    ///
    /// The exact player tile is admitted first by `synchronize_async`. This
    /// method uses the same worker/archive owner for remaining declared ADTs.
    /// A pending tile remains unavailable until all of its assets are prepared.
    /// Map replacement retires the job even if the next map/tile keys match.
    ///
    /// # Errors
    /// Returns [`RuntimeTerrainError`] when a required generation fails to
    /// prepare or the bounded worker cannot accept its task.
    pub fn synchronize_streaming_async(
        &mut self,
        map_id: u32,
        origin: Vec3,
        window: TerrainStreamingWindow,
        cpu: &CpuExecutor,
    ) -> Result<RuntimeTerrainStreamPoll, RuntimeTerrainError> {
        self.poll_stream_completion()?;
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.map_id() == map_id)
        else {
            return Ok(RuntimeTerrainStreamPoll::Idle);
        };
        if active.global_world_model.is_some() {
            return Ok(RuntimeTerrainStreamPoll::Current);
        }
        if self
            .streaming
            .as_ref()
            .is_none_or(|demand| demand.origin != origin || demand.window != window)
        {
            let previous_count = active.nearby.len();
            active.nearby.retain(|tile| window.contains(tile.index()));
            if active.nearby.len() != previous_count {
                active.synchronize_movement_owners();
            }
            // 0x007B5950 registers every missing WDT owner before sorting its
            // request array. 0x007D9A8A appends the reference to the persistent
            // list through 0x006DED60. These entries survive CPU load progress
            // and player-tile promotion, including equal-distance sort ties.
            let mut owners = self.streaming.as_ref().map_or_else(Vec::new, |demand| {
                demand
                    .owners
                    .iter()
                    .copied()
                    .filter(|&tile| window.contains(tile))
                    .collect()
            });
            for tile in active
                .tile
                .iter()
                .chain(&active.nearby)
                .map(ResidentTerrainTile::index)
            {
                if window.contains(tile) && !owners.contains(&tile) {
                    owners.push(tile);
                }
            }
            for tile in window.tiles() {
                if active.terrain.tile(tile).exists() && !owners.contains(&tile) {
                    owners.push(tile);
                }
            }
            let mut tiles = owners.clone();
            prioritize_terrain_tiles(&mut tiles, origin)?;
            self.streaming = Some(TerrainStreamingDemand {
                origin,
                window,
                owners,
                tiles,
            });
        }
        let demand = self
            .streaming
            .as_ref()
            .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
        let mut missing = demand
            .tiles
            .iter()
            .copied()
            .filter(|&tile| active.tile_at(tile).is_none());
        let next = missing.next();
        let remaining_tiles = usize::from(next.is_some()) + missing.count();
        let Some(tile) = next else {
            return Ok(RuntimeTerrainStreamPoll::Current);
        };
        if self.pending.is_none()
            && self.pending_stream.is_none()
            && !self.failed_stream.contains(&tile)
        {
            let definition = self
                .maps
                .map(map_id)
                .cloned()
                .ok_or(RuntimeTerrainError::UnknownMap { map_id })?;
            let source = self.take_worker_source()?;
            let request = TerrainRequest { map_id, tile };
            let task =
                cpu.try_submit(move || prepare_terrain_on_worker(source, definition, request))?;
            self.pending_stream = Some(PendingTerrainGeneration {
                request,
                submitted_at: Instant::now(),
                retain_without_world: false,
                eligible_for_publication: true,
                task,
            });
        }
        Ok(RuntimeTerrainStreamPoll::Pending { remaining_tiles })
    }

    /// Returns an admitted ADT by address, including a retained neighboring tile.
    #[must_use]
    pub fn resident_tile_at(&self, index: TerrainTileIndex) -> Option<&DecodedTerrainTile> {
        self.active
            .as_ref()?
            .tile_at(index)
            .map(|tile| &tile.decoded)
    }

    /// Returns the number of complete resident ADT generations in this world.
    #[must_use]
    pub fn resident_tile_count(&self) -> usize {
        self.active.as_ref().map_or(0, |active| {
            usize::from(active.tile.is_some()) + active.nearby.len()
        })
    }

    /// Joins only a completed job and publishes it into its original world.
    pub(super) fn poll_stream_completion(&mut self) -> Result<(), RuntimeTerrainError> {
        if self
            .pending_stream
            .as_ref()
            .is_none_or(|pending| !pending.task.is_finished())
        {
            return Ok(());
        }
        let Some(pending) = self.pending_stream.take() else {
            return Ok(());
        };
        let completion = pending.task.join()?;
        if let Some(worker) = completion.worker {
            self.worker = Some(worker);
        }
        let Some(active) = self.active.as_mut().filter(|active| {
            pending.eligible_for_publication && active.map_id() == pending.request.map_id
        }) else {
            return Ok(());
        };
        if self
            .streaming
            .as_ref()
            .is_none_or(|demand| !demand.window.contains(pending.request.tile))
        {
            return Ok(());
        }
        match completion.result {
            Ok(mut resident) => {
                if let Some(tile) = resident.tile.take()
                    && active.tile_at(tile.index()).is_none()
                {
                    if let Err(error) = active.validate_tile_placements(&tile) {
                        self.failed_stream.insert(pending.request.tile);
                        return Err(error);
                    }
                    active.nearby.push(tile);
                    active.synchronize_movement_owners();
                }
            }
            Err(error) => {
                self.failed_stream.insert(pending.request.tile);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Retires demand immediately while retaining the live task for later join.
    pub(super) fn retire_streaming(&mut self) {
        self.streaming = None;
        self.failed_stream.clear();
        if let Some(pending) = self.pending_stream.as_mut() {
            pending.eligible_for_publication = false;
        }
    }

    /// Reuses a resident neighbor when authoritative movement crosses an ADT edge.
    pub(super) fn promote_resident_tile(&mut self, request: TerrainRequest) -> bool {
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.map_id() == request.map_id)
        else {
            return false;
        };
        let Some(index) = active
            .nearby
            .iter()
            .position(|tile| tile.index() == request.tile)
        else {
            return false;
        };
        let tile = active.nearby.remove(index);
        if let Some(previous) = active.tile.replace(tile) {
            active.nearby.push(previous);
        }
        self.failed_request = None;
        true
    }

    /// Publishes a new primary tile while retaining same-world neighboring owners.
    pub(super) fn publish_active(
        &mut self,
        mut resident: ResidentTerrainMap,
    ) -> Result<(), RuntimeTerrainError> {
        if let Some(previous) = self
            .active
            .as_ref()
            .filter(|previous| previous.map_id() == resident.map_id())
            && let Some(tile) = resident.tile.as_ref()
        {
            previous.validate_tile_placements(tile)?;
        }
        if let Some(previous) = self.active.take()
            && previous.map_id() == resident.map_id()
            && resident.global_world_model.is_none()
            && let Some(demand) = self.streaming.as_ref()
        {
            resident.movement = previous.movement;
            for tile in previous.tile.into_iter().chain(previous.nearby) {
                if demand.window.contains(tile.index()) && resident.tile_at(tile.index()).is_none()
                {
                    resident.nearby.push(tile);
                }
            }
        }
        resident.synchronize_movement_owners();
        self.active = Some(resident);
        Ok(())
    }
}

impl ResidentTerrainMap {
    /// Extends per-ADT placement validation across the shared world identity domain.
    pub(super) fn validate_tile_placements(
        &self,
        added: &ResidentTerrainTile,
    ) -> Result<(), RuntimeTerrainError> {
        // The admitted scenes identify selected MCRF records; unreferenced file
        // records have no live owner and must not create a false conflict.
        let mut doodads = HashMap::new();
        let mut world_models = HashMap::new();
        for tile in self
            .tile
            .iter()
            .chain(&self.nearby)
            .chain(std::iter::once(added))
        {
            let tile_doodads = tile
                .decoded
                .chunks()
                .iter()
                .flat_map(|chunk| chunk.doodad_references())
                .map(|&index| &tile.decoded.doodads()[index as usize])
                .map(|placement| (placement.unique_id(), placement))
                .collect::<HashMap<_, _>>();
            let tile_world_models = tile
                .decoded
                .chunks()
                .iter()
                .flat_map(|chunk| chunk.world_model_references())
                .map(|&index| &tile.decoded.world_models()[index as usize])
                .map(|placement| (placement.unique_id(), placement))
                .collect::<HashMap<_, _>>();
            for placement in tile_doodads.values().copied() {
                let unique_id = placement.unique_id();
                if let Some(previous) = doodads.insert(unique_id, placement)
                    && !super::same_doodad_placement(previous, placement)
                {
                    return Err(RuntimeTerrainError::ConflictingDoodadPlacement { unique_id });
                }
            }
            for placement in tile_world_models.values().copied() {
                let unique_id = placement.unique_id();
                if let Some(previous) = world_models.insert(unique_id, placement)
                    && !super::world_model_residency::same_world_model_placement(
                        previous, placement,
                    )
                {
                    return Err(RuntimeTerrainError::ConflictingWorldModelPlacement { unique_id });
                }
            }
        }
        Ok(())
    }

    /// Resolves a complete CPU generation without conflating absence with WDT holes.
    pub(super) fn tile_at(&self, index: TerrainTileIndex) -> Option<&ResidentTerrainTile> {
        self.tile
            .iter()
            .chain(&self.nearby)
            .find(|tile| tile.index() == index)
    }
}

impl ResidentTerrainTile {
    /// Shares the immutable plan with renderer culling for this generation.
    pub(in crate::application) fn mesh(
        &self,
    ) -> &std::sync::Arc<solarity_rendering::TerrainTileMeshPlan> {
        &self.mesh
    }

    pub(in crate::application) fn textures(
        &self,
    ) -> &[std::sync::Arc<solarity_asset::BlpTextureSource>] {
        &self.textures
    }

    pub(in crate::application) fn m2_scene(&self) -> &super::m2_residency::ResidentM2Scene {
        &self.m2_scene
    }

    pub(in crate::application) fn world_models(
        &self,
    ) -> &super::world_model_residency::ResidentWorldModelScene {
        &self.world_models
    }
}
