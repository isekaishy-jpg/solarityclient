//! A backdrop retains its private bank through primary and material source turns.
use super::super::load_glue_model_generation;
use super::{
    BackdropArchiveOwner, BackdropArchiveState, BackdropCompletion, BackdropModel,
    GlueBackdropAssets, RuntimeGlueModelError,
};
use crate::application::terrain_coordinator::SharedTerrainSources;
use solarity_asset::{AssetStore, BlpTextureCache, BlpTexturePreparation, M2LoadError};
use solarity_cpu::{CpuError, CpuTaskStep, JobContext};
use std::{ops::ControlFlow, sync::Arc};

impl BackdropArchiveOwner {
    /// Selected/prewarm demand belongs to the existing operation, including suspended textures.
    pub(super) fn steps(
        self,
        model: BackdropModel,
        shared: SharedTerrainSources,
    ) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<BackdropCompletion> + Send {
        let mut owner = Some(self);
        let mut input = Some(model);
        let mut decoded = None;
        let mut textures = BlpTexturePreparation::default();
        move |context| {
            context.diagnostic_value("glue.backdrop.archive_request", 1);
            let assets = owner
                .as_mut()
                .unwrap_or_else(|| unreachable!("backdrop operation retains its bank"));
            let result =
                if context.is_cancelled() && !matches!(input, Some(BackdropModel::Producer(_))) {
                    Err(Arc::new(RuntimeGlueModelError::from(
                        CpuError::JobCancelled,
                    )))
                } else {
                    if assets.state.is_none() {
                        assets.state = Some(
                            AssetStore::mount(assets.catalog.clone())
                                .map(|store| BackdropArchiveState {
                                    store,
                                    textures: BlpTextureCache::new(),
                                })
                                .map_err(Arc::new),
                        );
                        return CpuTaskStep::Continue;
                    }
                    match assets
                        .state
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("backdrop mount has a terminal outcome"))
                    {
                        Err(error) => {
                            let error = M2LoadError::Asset(Arc::clone(error));
                            if let Some(BackdropModel::Producer(producer)) = input.take() {
                                producer.fail(error.clone());
                            }
                            Err(Arc::new(RuntimeGlueModelError::SharedModel(error)))
                        }
                        Ok(state) => {
                            if let Some(input) = input.take() {
                                let model = match input {
                                    BackdropModel::Ready(model) => Ok(model),
                                    BackdropModel::Producer(producer) => producer
                                        .load_admitted(&mut state.store, &shared.read_budget()),
                                };
                                match model {
                                    Ok(model) => {
                                        decoded = Some(model);
                                        return CpuTaskStep::Continue;
                                    }
                                    Err(error) => {
                                        return CpuTaskStep::Complete(BackdropCompletion {
                                            assets: owner.take().unwrap_or_else(|| {
                                                unreachable!("backdrop bank stays owned")
                                            }),
                                            result: Err(Arc::new(
                                                RuntimeGlueModelError::SharedModel(error),
                                            )),
                                        });
                                    }
                                }
                            }
                            let model = decoded.as_ref().unwrap_or_else(|| {
                                unreachable!("backdrop materials follow primary source")
                            });
                            match shared.materials(
                                &mut textures,
                                &mut state.textures,
                                &mut state.store,
                                |textures, store| {
                                    load_glue_model_generation(model.clone(), textures, store)
                                },
                            ) {
                                Ok(ControlFlow::Continue(edge)) => return CpuTaskStep::Wait(edge),
                                Ok(ControlFlow::Break((model, textures))) => {
                                    Ok(Arc::new(GlueBackdropAssets { model, textures }))
                                }
                                Err(error) => Err(Arc::new(error)),
                            }
                        }
                    }
                };
            if let Some(Ok(state)) = &mut assets.state {
                state.textures.collect_unused();
            }
            CpuTaskStep::Complete(BackdropCompletion {
                assets: owner
                    .take()
                    .unwrap_or_else(|| unreachable!("backdrop bank stays owned")),
                result,
            })
        }
    }
}
