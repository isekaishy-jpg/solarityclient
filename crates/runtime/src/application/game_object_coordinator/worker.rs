//! Bounded worker ownership shared by every visible GameObject resource.

use super::world_model::GameObjectWorldModelSource;
use super::{
    GameObjectResource, ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectResourceKind,
};
use crate::application::liquid::LiquidAssetCache;
use crate::application::terrain_coordinator::RuntimeTerrainError;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelCache;
use solarity_asset::{ArchiveCatalog, AssetStore, BlpTextureCache, M2ModelCache};

pub(super) enum GameObjectWorkerSource {
    Catalog(ArchiveCatalog),
    Ready(Box<GameObjectWorkerState>),
}

pub(super) struct GameObjectWorkerState {
    assets: AssetStore,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: ResidentWorldModelCache,
    liquid_assets: LiquidAssetCache,
}

impl GameObjectWorkerState {
    fn mount(catalog: ArchiveCatalog) -> Result<Self, RuntimeGameObjectError> {
        Ok(Self {
            assets: AssetStore::mount(catalog).map_err(RuntimeTerrainError::from)?,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: ResidentWorldModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
        })
    }

    fn prepare(
        &mut self,
        request: &ResourceRequest,
    ) -> Result<GameObjectResource, RuntimeGameObjectError> {
        match request.kind {
            RuntimeGameObjectResourceKind::M2 => {
                Ok(GameObjectResource::M2(ResidentM2Source::load(
                    &request.path,
                    &mut self.models,
                    &mut self.textures,
                    &mut self.assets,
                )?))
            }
            RuntimeGameObjectResourceKind::WorldModel => Ok(GameObjectResource::WorldModel(
                GameObjectWorldModelSource::load(
                    &request.path,
                    &mut self.world_models,
                    &mut self.models,
                    &mut self.textures,
                    &mut self.liquid_assets,
                    &mut self.assets,
                )?,
            )),
        }
    }

    fn collect_unused(&mut self) {
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }
}

pub(super) struct GameObjectWorkerCompletion {
    pub(super) worker: Option<Box<GameObjectWorkerState>>,
    pub(super) result: Result<GameObjectResource, RuntimeGameObjectError>,
}

pub(super) fn prepare_on_worker(
    source: GameObjectWorkerSource,
    request: &ResourceRequest,
) -> GameObjectWorkerCompletion {
    let mut worker = match source {
        GameObjectWorkerSource::Catalog(catalog) => match GameObjectWorkerState::mount(catalog) {
            Ok(worker) => Box::new(worker),
            Err(source) => {
                return GameObjectWorkerCompletion {
                    worker: None,
                    result: Err(source),
                };
            }
        },
        GameObjectWorkerSource::Ready(worker) => worker,
    };
    let result = worker.prepare(request);
    worker.collect_unused();
    GameObjectWorkerCompletion {
        worker: Some(worker),
        result,
    }
}
