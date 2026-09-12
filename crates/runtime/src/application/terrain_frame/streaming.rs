//! Incremental renderer publication of complete resident ADT generations.

use std::sync::Arc;

use solarity_asset::TerrainTileIndex;
use solarity_rendering::VulkanRenderer;

use crate::application::frame_profile::RuntimeFrameProfile;
use crate::application::terrain_coordinator::ResidentTerrainTile;
use crate::random::CrtRand;

use super::{RuntimeTerrainFrameError, TerrainFrame, TerrainGpuTile, prepare_tile_draws};

impl TerrainFrame {
    /// Services CPU-only detail retirement after frame publication. Saturation
    /// retains ownership for the next frame instead of blocking or freeing inline.
    pub(in crate::application) fn service_cpu_retirements(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), solarity_cpu::CpuError> {
        self.ground_detail.service_retirements(cpu)
    }

    /// Consumes this world frame and queues its ADTs for fence-covered destruction.
    pub(in crate::application) fn retire(
        self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let handles = self
            .tiles
            .iter()
            .flat_map(|tile| &tile.liquids)
            .map(|batch| batch.mesh())
            .collect::<Vec<_>>();
        renderer.retire_liquid_meshes(&handles)?;
        self.world_models.retire_liquids(renderer)?;
        renderer.retire_terrain_plans(self.tiles.iter().map(|tile| tile.plan.as_ref()))?;
        Ok(())
    }

    /// Whether new terrain belongs to this frame's still-active tiled world.
    pub(in crate::application) fn belongs_to_map(&self, map_id: u32) -> bool {
        self.map_id == Some(map_id)
    }

    /// Publishes added tiles and retires departed owners without resetting live M2s.
    pub(in crate::application) fn synchronize_tiles<'a>(
        &mut self,
        renderer: &mut VulkanRenderer,
        primary: TerrainTileIndex,
        tiles: impl Iterator<Item = &'a ResidentTerrainTile> + Clone,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        // Arc identity distinguishes a reloaded ADT from an older generation at
        // the same address. Merely promoting a retained neighbor changes no GPU
        // resources and consumes no new animation randomness.
        self.tile = Some(primary);
        if self.tiles.len() == tiles.clone().count()
            && self.tiles.iter().all(|gpu| {
                tiles
                    .clone()
                    .any(|tile| Arc::ptr_eq(&gpu.plan, tile.mesh()))
            })
        {
            return Ok(());
        }
        // Sample only residency changes, so steady frames cannot dilute upload stalls.
        // 7831A0 invalidates cached maps when old and new terrain coverage is
        // disjoint. A forced replacement of every generation also needs fresh
        // maps, as in 7BD9F0's synchronous residency refresh.
        if !self.tiles.iter().any(|gpu| {
            tiles
                .clone()
                .any(|tile| Arc::ptr_eq(&gpu.plan, tile.mesh()))
        }) {
            self.environment_shadows = None;
        }
        let mut profile = RuntimeFrameProfile::new("Terrain publication");
        let mut added = Vec::new();
        for tile in tiles.clone() {
            if !self
                .tiles
                .iter()
                .any(|gpu| Arc::ptr_eq(&gpu.plan, tile.mesh()))
            {
                added.push(TerrainGpuTile {
                    draws: prepare_tile_draws(renderer, tile.mesh(), tile.textures())?,
                    plan: Arc::clone(tile.mesh()),
                    liquids: self
                        .liquid_materials
                        .prepare_terrain(renderer, tile.liquid_batches())?,
                });
            }
        }
        profile.mark("terrain and liquids");
        self.m2.synchronize_static_scenes(
            renderer,
            tiles.clone().map(ResidentTerrainTile::m2_scene),
            random,
        )?;
        profile.mark("M2 membership");
        self.world_models.synchronize_static_scenes(
            renderer,
            tiles.clone().map(ResidentTerrainTile::world_models),
        )?;
        profile.mark("WMO membership");
        let departed_liquids = self
            .tiles
            .iter()
            .filter(|gpu| {
                !tiles
                    .clone()
                    .any(|tile| Arc::ptr_eq(&gpu.plan, tile.mesh()))
            })
            .flat_map(|tile| &tile.liquids)
            .map(|batch| batch.mesh())
            .collect::<Vec<_>>();
        renderer.retire_liquid_meshes(&departed_liquids)?;
        renderer.retire_terrain_plans(
            self.tiles
                .iter()
                .filter(|gpu| {
                    !tiles
                        .clone()
                        .any(|tile| Arc::ptr_eq(&gpu.plan, tile.mesh()))
                })
                .map(|gpu| gpu.plan.as_ref()),
        )?;
        self.tiles.retain(|gpu| {
            tiles
                .clone()
                .any(|tile| Arc::ptr_eq(&gpu.plan, tile.mesh()))
        });
        self.tiles.extend(added);
        profile.mark("retirement");
        Ok(())
    }
}
