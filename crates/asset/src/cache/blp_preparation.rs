//! Private construction retries preserve ordered source outcomes and suspend only on readiness.
use crate::{
    AssetError, AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad, BlpLoadDependency,
    BlpLoadError, BlpTextureCache, BlpTextureSource,
};
use solarity_cpu::{
    CpuService, CpuServiceControl, CpuStorageBudget, CpuStorageClass, CpuTaskDependency,
};
use std::{ops::ControlFlow, sync::Arc};

/// A domain retains immutable construction inputs across discovered texture dependencies.
/// The operation may update decode caches, but must publish scene/gameplay changes only on success.
/// Successful sources stay in the supplied cache; failed authored sources retain their exact
/// result for this construction, so retrying its prefix does not repeat reads or change decisions.
#[derive(Default)]
pub struct BlpTexturePreparation {
    turn: Option<SourceTurn>,
}
#[derive(Default)]
pub(super) struct SourceTurn {
    service: Option<CpuServiceControl>,
    budget: Option<CpuStorageBudget>,
    failed: Vec<(AssetResourceKey, BlpLoadError)>,
    pending: Option<(AssetResourceKey, BlpLoadDependency)>,
    suspension: Option<CpuTaskDependency>,
}
impl BlpTexturePreparation {
    /// Runs private construction until completion or the first shared source dependency.
    /// The caller retains its original inputs and invokes this again only after readiness.
    /// No executor or worker wait is created here; the admitted caller supplies live demand.
    /// # Errors
    /// Returns the builder's original failure or source admission/producer failure.
    /// # Panics
    /// Rejects nested construction on the same cache, or resumption before readiness.
    pub fn run<T, E: From<AssetError>>(
        &mut self,
        cache: &mut BlpTextureCache,
        store: &mut AssetStore,
        budget: &CpuStorageBudget,
        service: &CpuServiceControl,
        operation: impl FnOnce(&mut BlpTextureCache, &mut AssetStore) -> Result<T, E>,
    ) -> Result<ControlFlow<T, CpuTaskDependency>, E> {
        assert!(
            cache.preparation.is_none(),
            "one texture construction owns a cache turn"
        );
        let mut turn = self.turn.take().unwrap_or_default();
        let policy = AssetReadBudget::for_service(budget.clone(), service.service());
        if let Some((key, dependency)) = turn.pending.take() {
            match dependency
                .poll()
                .unwrap_or_else(|| unreachable!("texture construction follows readiness"))
            {
                Ok(source) => {
                    store.with_read_budget(&policy, |store| cache.adopt(store, source))?;
                }
                Err(error) => turn.failed.push((key, error)),
            }
        }
        turn.budget = Some(budget.clone());
        turn.service = Some(service.clone());
        cache.preparation = Some(turn);
        let scope = ConstructionScope { cache, owner: self };
        let result = store.with_read_budget(&policy, |store| operation(scope.cache, store));
        drop(scope);
        let turn = self
            .turn
            .as_mut()
            .unwrap_or_else(|| unreachable!("construction restores its source state"));
        if let Some(edge) = turn.suspension.take() {
            drop(result);
            return Ok(ControlFlow::Continue(edge));
        }
        self.turn = None;
        result.map(ControlFlow::Break)
    }
}
/// Panic/error paths restore the caller's construction and never leak policy to later loads.
struct ConstructionScope<'a> {
    cache: &'a mut BlpTextureCache,
    owner: &'a mut BlpTexturePreparation,
}
impl Drop for ConstructionScope<'_> {
    fn drop(&mut self) {
        self.owner.turn = self.cache.preparation.take();
    }
}

pub(super) fn load(
    cache: &mut BlpTextureCache,
    store: &mut AssetStore,
    key: AssetResourceKey,
) -> Result<Arc<BlpTextureSource>, AssetError> {
    let turn = cache
        .preparation
        .as_ref()
        .unwrap_or_else(|| unreachable!("shared load has a construction owner"));
    if turn.suspension.is_some() {
        return Err(AssetError::TexturePending);
    }
    if let Some((_, error)) = turn.failed.iter().find(|(old, _)| old == &key) {
        return Err(AssetError::TextureRequest(error.clone()));
    }
    let budget = turn
        .budget
        .as_ref()
        .unwrap_or_else(|| unreachable!("admitted construction has a budget"))
        .clone();
    let service = turn
        .service
        .as_ref()
        .unwrap_or_else(|| unreachable!("admitted construction has demand"))
        .clone();
    let result = match store
        .texture_cache_service()
        .request_for(&key, service.service())
    {
        BlpLoad::Ready(source) => Ok(source),
        BlpLoad::Pending(request) => {
            let class = if service.service() == CpuService::Speculative {
                CpuStorageClass::Speculative
            } else {
                CpuStorageClass::Required
            };
            let dependency = request.dependency(&budget, class)?;
            let edge = dependency.task_dependency()?;
            let turn = cache
                .preparation
                .as_mut()
                .unwrap_or_else(|| unreachable!("construction remains owned"));
            turn.pending = Some((key, dependency));
            turn.suspension = Some(edge);
            return Err(AssetError::TexturePending);
        }
        BlpLoad::Producer(producer) => {
            let priority = match service.scoped_demand(CpuService::Speculative) {
                Ok(priority) => priority,
                Err(error) => {
                    let error = BlpLoadError::from(error);
                    producer.fail(error.clone());
                    return Err(error.into());
                }
            };
            let demand = producer.subscribe_for(CpuService::Speculative);
            assert!(
                demand.bind_service(priority.control()),
                "one admitted texture producer binds demand"
            );
            producer.load(
                store,
                &AssetReadBudget::for_service(budget, priority.control().service()),
            )
        }
    };
    match result {
        Ok(source) => cache.adopt(store, source),
        Err(error) => {
            cache
                .preparation
                .as_mut()
                .unwrap_or_else(|| unreachable!("construction remains owned"))
                .failed
                .push((key, error.clone()));
            Err(error.into())
        }
    }
}
