//! Owned archive/cache state returned by contextual terrain continuations.

mod preparation;
pub(super) use preparation::terrain_steps;

#[cfg(test)]
#[path = "../../../../tests/application/terrain_worker_steps.rs"]
mod tests;

use super::{
    ResidentTerrainMap, RuntimeTerrainError, ground_detail::GroundDetailAssetCache,
    world_model_residency::ResidentWorldModelCache,
};
use crate::application::liquid::LiquidAssetCache;
use solarity_asset::{ArchiveCatalog, AssetStore, BlpTextureCache, M2ModelCache};
use solarity_rendering::TerrainLowDetailMap;
use std::sync::Arc;

/// Moves the private archive/cache bank only after the caller reserves CPU admission.
pub(super) enum TerrainWorkerSource {
    Catalog(ArchiveCatalog),
    Ready(Box<TerrainWorkerState>),
}

/// One bank survives tile jobs; no cache mutex spans decoding or a service yield.
pub(super) struct TerrainWorkerState {
    // The inner absence remembers an absent WDL for the current map.
    low_detail: Option<(u32, Option<Arc<TerrainLowDetailMap>>)>,
    assets: AssetStore,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: ResidentWorldModelCache,
    liquid_assets: LiquidAssetCache,
    ground_detail_assets: GroundDetailAssetCache,
}

impl TerrainWorkerState {
    /// A partially mounted archive stack never enters the reusable bank.
    fn new(assets: AssetStore) -> Self {
        Self {
            assets,
            low_detail: None,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: ResidentWorldModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
            ground_detail_assets: GroundDetailAssetCache::default(),
        }
    }

    /// Terminal cleanup follows both success and domain failure, as before staging.
    fn collect_unused(&mut self) {
        self.textures.collect_unused();
        self.world_models.collect_unused();
    }
}

/// Only a terminal generation can cross the existing main-thread publication gate.
pub(super) struct TerrainWorkerCompletion {
    pub(super) worker: Option<Box<TerrainWorkerState>>,
    /// None is cooperative withdrawal, never a publishable partial generation.
    pub(super) result: Result<Option<ResidentTerrainMap>, RuntimeTerrainError>,
}
