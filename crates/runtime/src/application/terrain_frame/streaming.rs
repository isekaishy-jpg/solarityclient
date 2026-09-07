//! Incremental renderer publication of complete resident ADT generations.

use std::sync::Arc;

use solarity_asset::TerrainTileIndex;
use solarity_rendering::VulkanRenderer;

use crate::application::terrain_coordinator::ResidentTerrainTile;
use crate::random::CrtRand;

use super::{RuntimeTerrainFrameError, TerrainFrame, TerrainGpuTile, prepare_tile_draws};

impl TerrainFrame {
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
        self.m2.synchronize_static_scenes(
            renderer,
            tiles.clone().map(ResidentTerrainTile::m2_scene),
            random,
        )?;
        self.world_models.synchronize_static_scenes(
            renderer,
            tiles.clone().map(ResidentTerrainTile::world_models),
        )?;
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
        Ok(())
    }
}
