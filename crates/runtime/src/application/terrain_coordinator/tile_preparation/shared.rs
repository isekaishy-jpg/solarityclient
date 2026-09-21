//! Shared primary-source results remain with their ordered terrain consumer.

use super::super::RuntimeTerrainError;
use solarity_asset::{
    AssetPath, AssetResourceKey, AssetStore, DecodedM2Model, M2Load, M2LoadDependency,
    ResourceLease,
};
use solarity_cpu::{CpuServiceControl, CpuStorageBudget, CpuStorageClass, CpuTaskDependency};
use std::ops::ControlFlow;

/// The admitted terrain service owns its budget and live scheduling identity.
#[derive(Clone)]
pub(in crate::application) struct SharedTerrainSources {
    pub(in crate::application) budget: CpuStorageBudget,
    pub(in crate::application) service: CpuServiceControl,
}

/// An ordered WMO consumer retains either another producer's edge or its own decode cursor.
pub(in crate::application) enum PendingWorldModel {
    Waiting(solarity_asset::WmoLoadDependency),
    Loading {
        producer: solarity_asset::WmoLoadProducer,
        // Source demand is additive to the containing terrain job and ends at publication.
        priority: solarity_cpu::CpuServiceScope,
        demand: solarity_asset::WmoLoadRequest,
        budget: CpuStorageBudget,
    },
}

impl PendingWorldModel {
    /// Withdrawal stops scene construction, but cannot cancel another owner's
    /// joined source. Finish only the already claimed producer, one stage per
    /// turn. Its terminal source error is published to joiners by the producer.
    /// A waiting consumer simply drops its own interest without waiting.
    pub(in crate::application) fn retire_step(
        pending: &mut Option<Self>,
        store: &mut AssetStore,
    ) -> bool {
        let Some(Self::Loading {
            mut producer,
            priority,
            demand,
            budget,
        }) = pending.take()
        else {
            return true;
        };
        match store.with_read_budget(
            &solarity_asset::AssetReadBudget::for_service(
                budget.clone(),
                priority.control().service(),
            ),
            |store| producer.step(store),
        ) {
            Ok(None) => {
                *pending = Some(Self::Loading {
                    producer,
                    priority,
                    demand,
                    budget,
                });
                false
            }
            Ok(Some(_)) | Err(_) => true,
        }
    }
}

impl SharedTerrainSources {
    /// Samples the job's current effective demand before each finite source turn.
    pub(in crate::application) fn read_budget(&self) -> solarity_asset::AssetReadBudget {
        solarity_asset::AssetReadBudget::for_service(self.budget.clone(), self.service.service())
    }
    /// Private material construction may suspend without publishing partial scene state.
    pub(in crate::application) fn materials<T, E: From<solarity_asset::AssetError>>(
        &self,
        pending: &mut solarity_asset::BlpTexturePreparation,
        cache: &mut solarity_asset::BlpTextureCache,
        store: &mut AssetStore,
        operation: impl FnOnce(&mut solarity_asset::BlpTextureCache, &mut AssetStore) -> Result<T, E>,
    ) -> Result<ControlFlow<T, CpuTaskDependency>, E> {
        pending.run(cache, store, &self.budget, &self.service, operation)
    }

    /// Texture consumers retain their local cache pins while sharing pending decode authority.
    pub(in crate::application) fn texture(
        &self,
        path: &AssetPath,
        pending: &mut Option<solarity_asset::BlpLoadDependency>,
        store: &mut AssetStore,
        cache: &mut solarity_asset::BlpTextureCache,
    ) -> Result<
        ControlFlow<std::sync::Arc<solarity_asset::BlpTextureSource>, CpuTaskDependency>,
        RuntimeTerrainError,
    > {
        let shared = crate::application::texture_source_job::SharedTextureSources {
            budget: self.budget.clone(),
            service: self.service.clone(),
        };
        match shared.load(store, path, pending)? {
            ControlFlow::Continue(edge) => Ok(ControlFlow::Continue(edge)),
            ControlFlow::Break(source) => Ok(ControlFlow::Break(
                store
                    .with_read_budget(&shared.read_budget(), |store| cache.adopt(store, source))?,
            )),
        }
    }

    /// Joins one namespace model. A new producer decodes in this admitted bulk
    /// turn; an existing producer supplies a readiness edge instead of a worker wait.
    pub(in crate::application) fn model(
        &self,
        path: &AssetPath,
        pending: &mut Option<M2LoadDependency>,
        store: &mut AssetStore,
    ) -> Result<ControlFlow<ResourceLease<DecodedM2Model>, CpuTaskDependency>, RuntimeTerrainError>
    {
        if let Some(dependency) = pending.take() {
            return Ok(ControlFlow::Break(dependency.poll().unwrap_or_else(
                || unreachable!("terrain source consumption follows dependency readiness"),
            )?));
        }
        let key = AssetResourceKey::new(store.namespace(), path.clone());
        match store
            .model_cache_service()
            .request_for(&key, self.service.service())?
        {
            M2Load::Ready(model) => Ok(ControlFlow::Break(model)),
            M2Load::Producer(producer) => Ok(ControlFlow::Break(producer.load_admitted(
                store,
                &solarity_asset::AssetReadBudget::for_service(
                    self.budget.clone(),
                    self.service.service(),
                ),
            )?)),
            M2Load::Pending(request) => {
                let dependency = request.dependency(&self.budget, CpuStorageClass::Required)?;
                let suspension = dependency.task_dependency()?;
                *pending = Some(dependency);
                Ok(ControlFlow::Continue(suspension))
            }
        }
    }
    /// Joins one root/group source, yielding between decode stages. Another
    /// producer supplies a readiness edge; this consumer never blocks a worker.
    pub(in crate::application) fn world_model(
        &self,
        path: &AssetPath,
        pending: &mut Option<PendingWorldModel>,
        store: &mut AssetStore,
    ) -> Result<
        ControlFlow<ResourceLease<solarity_asset::DecodedWorldModel>, Option<CpuTaskDependency>>,
        RuntimeTerrainError,
    > {
        if let Some(state) = pending.take() {
            return match state {
                PendingWorldModel::Waiting(dependency) => {
                    Ok(ControlFlow::Break(dependency.poll().unwrap_or_else(
                        || unreachable!("WMO consumption follows readiness"),
                    )?))
                }
                PendingWorldModel::Loading {
                    mut producer,
                    priority,
                    demand,
                    budget,
                } => match store.with_read_budget(
                    &solarity_asset::AssetReadBudget::for_service(
                        budget.clone(),
                        priority.control().service(),
                    ),
                    |store| producer.step(store),
                )? {
                    Some(model) => Ok(ControlFlow::Break(model)),
                    None => {
                        *pending = Some(PendingWorldModel::Loading {
                            producer,
                            priority,
                            demand,
                            budget,
                        });
                        Ok(ControlFlow::Continue(None))
                    }
                },
            };
        }
        let key = AssetResourceKey::new(store.namespace(), path.clone());
        match store
            .world_model_cache_service()
            .request_for(&key, self.service.service())?
        {
            solarity_asset::WmoLoad::Ready(model) => Ok(ControlFlow::Break(model)),
            solarity_asset::WmoLoad::Producer(producer) => {
                // Parent demand already follows this admitted job. This source's
                // own pin adds only speculative demand; other consumers may promote
                // it without owning or demoting unrelated terrain stages.
                let priority = match self
                    .service
                    .scoped_demand(solarity_cpu::CpuService::Speculative)
                {
                    Ok(priority) => priority,
                    Err(error) => {
                        let error = solarity_asset::WmoLoadError::Cpu(std::sync::Arc::new(error));
                        producer.fail(error.clone());
                        return Err(error.into());
                    }
                };
                let (producer, demand) =
                    producer.subscribe_owned(solarity_cpu::CpuService::Speculative)?;
                assert!(
                    demand.bind_service(priority.control()),
                    "one admitted WMO producer binds its source demand"
                );
                *pending = Some(PendingWorldModel::Loading {
                    producer,
                    priority,
                    demand,
                    budget: self.budget.clone(),
                });
                Ok(ControlFlow::Continue(None))
            }
            solarity_asset::WmoLoad::Pending(request) => {
                let dependency = request.dependency(&self.budget, CpuStorageClass::Required)?;
                let suspension = dependency.task_dependency()?;
                *pending = Some(PendingWorldModel::Waiting(dependency));
                Ok(ControlFlow::Continue(Some(suspension)))
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/shared_world_source_priority.rs"]
mod tests;
