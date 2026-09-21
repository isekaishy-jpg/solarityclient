//! Bounded worker ownership shared by every visible GameObject resource.

use super::super::{GameObjectResource, RuntimeGameObjectError};
use crate::application::liquid::LiquidAssetCache;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelCache;
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, BlpTextureCache, DecodedM2Model, M2LoadError,
    M2LoadProducer, ResourceLease,
};
use std::sync::Arc;

/// A worker receives useful producer work or an already published immutable model.
pub(in super::super) enum GameObjectM2Input {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
    Waiting(solarity_asset::M2LoadDependency),
}

pub(in super::super) enum GameObjectWorkerSource {
    Catalog(ArchiveCatalog),
    Ready(Box<GameObjectWorkerState>),
}

pub(in super::super) struct GameObjectWorkerState {
    pub(super) assets: AssetStore,
    pub(super) textures: BlpTextureCache,
    pub(super) world_models: ResidentWorldModelCache,
    pub(super) liquid_assets: LiquidAssetCache,
}

impl GameObjectWorkerState {
    pub(super) fn mount(catalog: ArchiveCatalog) -> Result<Self, Arc<AssetError>> {
        Ok(Self {
            assets: AssetStore::mount(catalog).map_err(Arc::new)?,
            textures: BlpTextureCache::new(),
            world_models: ResidentWorldModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
        })
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

/// Source and material readiness share one continuation and always return the owned bank.
pub(in super::super) fn model_steps(
    source: GameObjectWorkerSource,
    input: GameObjectM2Input,
    shared: crate::application::terrain_coordinator::SharedTerrainSources,
) -> impl FnMut(&solarity_cpu::JobContext<'_>) -> solarity_cpu::CpuTaskStep<GameObjectWorkerCompletion>
+ Send {
    use solarity_cpu::{CpuError, CpuTaskStep};
    use std::ops::ControlFlow;
    let mut source = Some(source);
    let mut input = Some(input);
    let mut worker: Option<Box<GameObjectWorkerState>> = None;
    let mut model = None;
    let mut materials = solarity_asset::BlpTexturePreparation::default();
    move |context| {
        context.diagnostic_value("game_object.prepare.model_step", 1);
        // An owned source producer still publishes for its other consumers on withdrawal.
        if context.is_cancelled() && !matches!(input, Some(GameObjectM2Input::Producer(_))) {
            let returned = worker.take().or_else(|| match source.take() {
                Some(GameObjectWorkerSource::Ready(bank)) => Some(bank),
                _ => None,
            });
            return CpuTaskStep::Complete(GameObjectWorkerCompletion {
                worker: returned,
                result: Err(CpuError::JobCancelled.into()),
            });
        }
        if worker.is_none() {
            worker = Some(
                match source
                    .take()
                    .unwrap_or_else(|| unreachable!("model work owns one bank"))
                {
                    GameObjectWorkerSource::Ready(bank) => bank,
                    GameObjectWorkerSource::Catalog(catalog) => {
                        match GameObjectWorkerState::mount(catalog) {
                            Ok(bank) => Box::new(bank),
                            Err(error) => {
                                let error = M2LoadError::Asset(error);
                                if let Some(GameObjectM2Input::Producer(producer)) = input.take() {
                                    producer.fail(error.clone());
                                }
                                return CpuTaskStep::Complete(GameObjectWorkerCompletion {
                                    worker: None,
                                    result: Err(error.into()),
                                });
                            }
                        }
                    }
                },
            );
            return CpuTaskStep::Continue;
        }
        let bank = worker
            .as_mut()
            .unwrap_or_else(|| unreachable!("mounted model bank stays owned"));
        let result = if model.is_none() {
            let decoded: Result<_, RuntimeGameObjectError> = match input
                .take()
                .unwrap_or_else(|| unreachable!("model input precedes materials"))
            {
                GameObjectM2Input::Ready(model) => Ok(model),
                GameObjectM2Input::Producer(producer) => producer
                    .load_admitted(&mut bank.assets, &shared.read_budget())
                    .map_err(Into::into),
                GameObjectM2Input::Waiting(dependency) => match dependency.poll() {
                    Some(result) => result.map_err(Into::into),
                    None => match dependency.task_dependency() {
                        Ok(edge) => {
                            input = Some(GameObjectM2Input::Waiting(dependency));
                            return CpuTaskStep::Wait(edge);
                        }
                        Err(error) => Err(error.into()),
                    },
                },
            };
            match decoded {
                Ok(decoded) => {
                    model = Some(decoded);
                    return CpuTaskStep::Continue;
                }
                Err(error) => Err(error),
            }
        } else {
            let decoded = model
                .as_ref()
                .unwrap_or_else(|| unreachable!("material work retains decoded input"));
            match shared.materials(
                &mut materials,
                &mut bank.textures,
                &mut bank.assets,
                |textures, store| {
                    ResidentM2Source::from_model_with_lights(
                        decoded.clone(),
                        textures,
                        store,
                        solarity_rendering::M2LocalLightCount::Four,
                    )
                },
            ) {
                Ok(ControlFlow::Continue(edge)) => return CpuTaskStep::Wait(edge),
                Ok(ControlFlow::Break(source)) => Ok(GameObjectResource::M2(source)),
                Err(error) => Err(error.into()),
            }
        };
        bank.collect_unused();
        CpuTaskStep::Complete(GameObjectWorkerCompletion {
            worker: worker.take(),
            result,
        })
    }
}
