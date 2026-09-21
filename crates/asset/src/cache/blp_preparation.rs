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
        let policy = AssetReadBudget::for_service(budget.clone(), service.service());
        self.run_owned(
            &mut (cache, store),
            |state| state.0,
            budget,
            service,
            |(cache, store)| store.with_read_budget(&policy, |store| operation(cache, store)),
        )
    }

    /// Runs construction through a private owner whose methods access its texture cache.
    /// The caller retains that cache across suspension and scopes archive reads to the
    /// supplied budget/service. Cache access must select the same bank for the entire call.
    /// # Errors
    /// Returns the original domain failure, preserving shared producer failures.
    /// # Panics
    /// Rejects nested construction on the same cache or resumption before readiness.
    pub fn run_owned<O, T, E>(
        &mut self,
        owner: &mut O,
        cache: fn(&mut O) -> &mut BlpTextureCache,
        budget: &CpuStorageBudget,
        service: &CpuServiceControl,
        operation: impl FnOnce(&mut O) -> Result<T, E>,
    ) -> Result<ControlFlow<T, CpuTaskDependency>, E> {
        assert!(
            cache(owner).preparation.is_none(),
            "one texture construction owns a cache turn"
        );
        let mut turn = self.turn.take().unwrap_or_default();
        turn.budget = Some(budget.clone());
        turn.service = Some(service.clone());
        cache(owner).preparation = Some(turn);
        let scope = ConstructionScope {
            owner,
            cache,
            preparation: self,
        };
        let result = operation(scope.owner);
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
struct ConstructionScope<'a, O> {
    owner: &'a mut O,
    cache: fn(&mut O) -> &mut BlpTextureCache,
    preparation: &'a mut BlpTexturePreparation,
}
impl<O> Drop for ConstructionScope<'_, O> {
    fn drop(&mut self) {
        self.preparation.turn = (self.cache)(self.owner).preparation.take();
    }
}

pub(super) fn load(
    cache: &mut BlpTextureCache,
    store: &mut AssetStore,
    key: AssetResourceKey,
) -> Result<Arc<BlpTextureSource>, AssetError> {
    // Consume the published outcome at its original authored position. Prefix cache hits
    // and retained failures are replayed before this edge, preserving first-error order.
    if cache.preparation.as_ref().is_some_and(|turn| {
        turn.suspension.is_none()
            && turn
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| *pending == key)
    }) {
        let turn = cache
            .preparation
            .as_mut()
            .unwrap_or_else(|| unreachable!("construction owns readiness"));
        let (_, dependency) = turn
            .pending
            .take()
            .unwrap_or_else(|| unreachable!("matching dependency stays owned"));
        match dependency
            .poll()
            .unwrap_or_else(|| unreachable!("texture construction follows readiness"))
        {
            Ok(source) => return cache.adopt(store, source),
            Err(error) => turn.failed.push((key.clone(), error)),
        }
    }
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
