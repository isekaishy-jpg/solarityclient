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
    Loading(solarity_asset::WmoLoadProducer),
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
        let Some(Self::Loading(mut producer)) = pending.take() else {
            return true;
        };
        match producer.step(store) {
            Ok(None) => {
                *pending = Some(Self::Loading(producer));
                false
            }
            Ok(Some(_)) | Err(_) => true,
        }
    }
}

impl SharedTerrainSources {
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
            M2Load::Producer(producer) => Ok(ControlFlow::Break(producer.load(store)?)),
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
                PendingWorldModel::Loading(mut producer) => match producer.step(store)? {
                    Some(model) => Ok(ControlFlow::Break(model)),
                    None => {
                        *pending = Some(PendingWorldModel::Loading(producer));
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
                *pending = Some(PendingWorldModel::Loading(producer));
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
