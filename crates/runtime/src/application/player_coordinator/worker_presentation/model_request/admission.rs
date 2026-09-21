//! Shared source demand is registered before transferring any appearance cache.

use super::super::super::{RuntimePlayerError, RuntimePlayerPresentation};
use super::super::AppearanceWorkerCache;
use super::work::{AppearanceBank, ModelInput};
use super::{AppearanceRequest, AppearanceTask};
use solarity_asset::{AssetResourceKey, DecodedM2Model, M2Load, ResourceLease};
use solarity_cpu::{CpuError, CpuExecutor, CpuService, CpuStorageClass};

impl<T: Send + 'static> AppearanceTask<T> {
    /// Register existing demand before CPU admission so saturation cannot prevent
    /// promotion. Refusal leaves the caller's exclusive bank in place.
    pub(in crate::application::player_coordinator) fn submit(
        cpu: &CpuExecutor,
        request: AppearanceRequest,
        cache: &mut Option<AppearanceWorkerCache>,
        prepare: impl FnOnce(
            &mut RuntimePlayerPresentation,
            ResourceLease<DecodedM2Model>,
        ) -> Result<T, RuntimePlayerError>
        + Send
        + 'static,
    ) -> Result<Option<Self>, RuntimePlayerError> {
        let key = AssetResourceKey::new(request.catalog.namespace(), request.model_path);
        let service = request.catalog.model_cache_service();
        let waiting = service.join_pending(&key, CpuService::Required)?;
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let load = match waiting {
            Some(request) => M2Load::Pending(request),
            None => service.request(&key)?,
        };
        let control = permit.service_control();
        let (model, demand) = match load {
            M2Load::Ready(model) => (ModelInput::Ready(model), None),
            M2Load::Producer(producer) => {
                let demand = producer.subscribe();
                assert!(
                    demand.bind_service(control.clone()),
                    "one producer binds each source task"
                );
                (ModelInput::Producer(producer), Some(demand))
            }
            M2Load::Pending(demand) => {
                let dependency = demand.dependency(cpu.storage(), CpuStorageClass::Required)?;
                (ModelInput::Pending(dependency), Some(demand))
            }
        };
        let bank = AppearanceBank {
            catalog: request.catalog,
            catalogs: request.catalogs,
            level: request.level,
            cache: cache
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?,
        };
        let task = permit.submit_resumable_with_context(bank.operation(
            model,
            request.sources,
            Box::new(prepare),
            cpu.storage().clone(),
            control,
        ));
        Ok(Some(Self::Direct { task, demand }))
    }
}
