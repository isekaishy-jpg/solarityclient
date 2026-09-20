//! Primary-source demand is registered before transferring any appearance cache.

use super::super::super::{RuntimePlayerError, RuntimePlayerPresentation};
use super::super::AppearanceWorkerCache;
use super::work::{AppearanceBank, ModelInput};
use super::{AppearanceRequest, AppearanceTask};
use solarity_asset::{AssetResourceKey, DecodedM2Model, M2Load, ResourceLease};
use solarity_cpu::{CpuError, CpuExecutor, CpuService};

impl<T: Send + 'static> AppearanceTask<T> {
    /// Pending sources gate the appearance without a worker wait. Capacity refusal
    /// returns None with the original cache bank still owned by the coordinator.
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
        let source_key = AssetResourceKey::new(request.catalog.namespace(), request.model_path);
        let service = request.catalog.model_cache_service();
        let (permit, model) =
            if let Some(waiting) = service.join_pending(&source_key, CpuService::Required)? {
                (None, M2Load::Pending(waiting))
            } else {
                let permit = match cpu.try_reserve() {
                    Ok(permit) => permit,
                    Err(CpuError::AtCapacity { .. }) => return Ok(None),
                    Err(error) => return Err(error.into()),
                };
                (Some(permit), service.request(&source_key)?)
            };
        let bank = AppearanceBank {
            catalog: request.catalog,
            catalogs: request.catalogs,
            level: request.level,
            cache: cache
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?,
        };
        let prepare = Box::new(prepare);
        let task = match model {
            M2Load::Pending(demand) => {
                drop(permit);
                let mut bank = Some(bank);
                match Self::dependent(cpu, &mut bank, demand, prepare) {
                    Ok(task) => task,
                    Err(error) => {
                        *cache = Some(
                            bank.take()
                                .unwrap_or_else(|| {
                                    unreachable!("refused appearance retains its bank")
                                })
                                .cache,
                        );
                        return match error {
                            CpuError::AtCapacity { .. } => Ok(None),
                            error => Err(error.into()),
                        };
                    }
                }
            }
            ready => {
                let (model, demand) = match ready {
                    M2Load::Ready(model) => (ModelInput::Ready(model), None),
                    M2Load::Producer(producer) => {
                        let demand = producer.subscribe();
                        (ModelInput::Producer(producer), Some(demand))
                    }
                    M2Load::Pending(_) => unreachable!("pending sources use dependency admission"),
                };
                let task = permit
                    .unwrap_or_else(|| unreachable!("new or ready sources reserve execution"))
                    .submit_with_context(move |context| {
                        context.diagnostic_value("appearance.prepare.direct", 1);
                        bank.prepare(model, prepare)
                    });
                if let Some(demand) = &demand {
                    assert!(
                        demand.bind_service(task.service_control()),
                        "one producer binds each source task"
                    );
                }
                Self::Direct { task, demand }
            }
        };
        Ok(Some(task))
    }
}
