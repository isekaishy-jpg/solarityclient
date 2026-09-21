//! Bounded worker ownership shared by every visible GameObject resource.

use super::super::world_model::GameObjectWorldModelSource;
use super::super::{
    GameObjectResource, ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectResourceKind,
};
use crate::application::liquid::LiquidAssetCache;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelCache;
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, BlpTextureCache, DecodedM2Model, M2LoadError,
    M2LoadProducer, M2ModelCache, ResourceLease,
};
use std::sync::Arc;

/// A worker receives useful producer work or an already published immutable model.
pub(in super::super) enum GameObjectM2Input {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
}

pub(in super::super) enum GameObjectWorkerSource {
    Catalog(ArchiveCatalog),
    Ready(Box<GameObjectWorkerState>),
}

pub(in super::super) struct GameObjectWorkerState {
    pub(super) assets: AssetStore,
    pub(super) textures: BlpTextureCache,
    models: M2ModelCache,
    pub(super) world_models: ResidentWorldModelCache,
    pub(super) liquid_assets: LiquidAssetCache,
}

impl GameObjectWorkerState {
    pub(super) fn mount(catalog: ArchiveCatalog) -> Result<Self, Arc<AssetError>> {
        Ok(Self {
            assets: AssetStore::mount(catalog).map_err(Arc::new)?,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: ResidentWorldModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
        })
    }

    fn prepare(
        &mut self,
        request: &ResourceRequest,
        model: Option<GameObjectM2Input>,
    ) -> Result<GameObjectResource, RuntimeGameObjectError> {
        match request.kind {
            RuntimeGameObjectResourceKind::M2 => {
                let model = match model
                    .unwrap_or_else(|| unreachable!("M2 dispatch carries source work"))
                {
                    GameObjectM2Input::Ready(model) => model,
                    GameObjectM2Input::Producer(producer) => producer.load(&mut self.assets)?,
                };
                Ok(GameObjectResource::M2(
                    ResidentM2Source::from_model_with_lights(
                        model,
                        &mut self.textures,
                        &mut self.assets,
                        solarity_rendering::M2LocalLightCount::Four,
                    )?,
                ))
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

    pub(super) fn collect_unused(&mut self) {
        self.textures.collect_unused();
        self.world_models.collect_unused();
    }
}

pub(in super::super) struct GameObjectWorkerCompletion {
    pub(in super::super) worker: Option<Box<GameObjectWorkerState>>,
    pub(in super::super) result: Result<GameObjectResource, RuntimeGameObjectError>,
}

pub(in super::super) fn prepare_on_worker(
    source: GameObjectWorkerSource,
    request: &ResourceRequest,
    model: Option<GameObjectM2Input>,
) -> GameObjectWorkerCompletion {
    let mut worker = match source {
        GameObjectWorkerSource::Catalog(catalog) => match GameObjectWorkerState::mount(catalog) {
            Ok(worker) => Box::new(worker),
            Err(source) => {
                let error = M2LoadError::Asset(source);
                if let Some(GameObjectM2Input::Producer(producer)) = model {
                    producer.fail(error.clone());
                }
                return GameObjectWorkerCompletion {
                    worker: None,
                    result: Err(error.into()),
                };
            }
        },
        GameObjectWorkerSource::Ready(worker) => worker,
    };
    let result = worker.prepare(request, model);
    worker.collect_unused();
    GameObjectWorkerCompletion {
        worker: Some(worker),
        result,
    }
}
