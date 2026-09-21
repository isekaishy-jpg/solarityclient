//! Shared texture prerequisites reuse admitted workers and yield on source readiness.
use solarity_asset::{
    AssetPath, AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad, BlpLoadDependency,
    BlpLoadError, BlpTextureSource,
};
use solarity_cpu::{
    CpuService, CpuServiceControl, CpuStorageBudget, CpuStorageClass, CpuTaskDependency,
};
use std::ops::ControlFlow;

#[derive(Clone)]
pub(super) struct SharedTextureSources {
    pub(super) budget: CpuStorageBudget,
    pub(super) service: CpuServiceControl,
}
impl SharedTextureSources {
    pub(super) fn read_budget(&self) -> AssetReadBudget {
        AssetReadBudget::for_service(self.budget.clone(), self.service.service())
    }
    /// The containing job retains its reader and cursor while a typed edge releases its worker.
    pub(super) fn load(
        &self,
        store: &mut AssetStore,
        path: &AssetPath,
        pending: &mut Option<BlpLoadDependency>,
    ) -> Result<ControlFlow<BlpTextureSource, CpuTaskDependency>, BlpLoadError> {
        let source = if let Some(dependency) = pending.take() {
            dependency
                .poll()
                .unwrap_or_else(|| unreachable!("texture consumption follows readiness"))?
        } else {
            let key = AssetResourceKey::new(store.namespace(), path.clone());
            match store
                .texture_cache_service()
                .request_for(&key, self.service.service())
            {
                BlpLoad::Ready(source) => source,
                BlpLoad::Pending(request) => {
                    let class = match self.service.service() {
                        CpuService::Speculative => CpuStorageClass::Speculative,
                        _ => CpuStorageClass::Required,
                    };
                    let dependency = request.dependency(&self.budget, class)?;
                    let edge = dependency.task_dependency()?;
                    *pending = Some(dependency);
                    return Ok(ControlFlow::Continue(edge));
                }
                BlpLoad::Producer(producer) => {
                    let priority = match self.service.scoped_demand(CpuService::Speculative) {
                        Ok(priority) => priority,
                        Err(error) => {
                            let error = BlpLoadError::from(error);
                            producer.fail(error.clone());
                            return Err(error);
                        }
                    };
                    let demand = producer.subscribe_for(CpuService::Speculative);
                    assert!(
                        demand.bind_service(priority.control()),
                        "one admitted texture producer binds its demand"
                    );
                    producer.load(
                        store,
                        &AssetReadBudget::for_service(
                            self.budget.clone(),
                            priority.control().service(),
                        ),
                    )?
                }
            }
        };
        source.admit(&self.read_budget())?;
        Ok(ControlFlow::Break(source))
    }
}
