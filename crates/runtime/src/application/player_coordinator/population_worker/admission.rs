//! Population primary-model admission joins the namespace's existing decode authority.

use super::super::{RuntimePlayerError, RuntimePlayerPresentation, RuntimePlayerSharedCatalogs};
use super::pending::{ModelInput, PendingTask, PopulationBank};
use super::{
    PendingPopulation, PopulationRequest, PopulationStage, PopulationWorker, PreparedPopulation,
};
use solarity_asset::{ArchiveCatalog, AssetResourceKey, DecodedM2Model, M2Load, ResourceLease};
use solarity_cpu::{CpuError, CpuExecutor, CpuService};

impl<K: PartialEq, T: PreparedPopulation> PopulationWorker<K, T> {
    /// Reserves execution before claiming a new decoder. Existing producers gate
    /// derived work without a worker-side wait or a duplicate source decode.
    pub(in crate::application::player_coordinator) fn submit(
        &mut self,
        cpu: &CpuExecutor,
        request: PopulationRequest<K>,
        catalog: ArchiveCatalog,
        catalogs: RuntimePlayerSharedCatalogs,
        prepare: impl FnOnce(
            &mut RuntimePlayerPresentation,
            ResourceLease<DecodedM2Model>,
        ) -> Result<T, RuntimePlayerError>
        + Send
        + 'static,
    ) -> Result<(), RuntimePlayerError> {
        if let Some(pending) = &mut self.pending {
            if pending.identity != request.identity
                || pending.key != request.key
                || pending.level != request.level
            {
                pending.withdraw();
            }
            return Ok(());
        }
        let source_key = AssetResourceKey::new(catalog.namespace(), request.model_path);
        let service = catalog.model_cache_service();
        let (permit, model) =
            if let Some(waiting) = service.join_pending(&source_key, CpuService::Required)? {
                (None, M2Load::Pending(waiting))
            } else {
                let permit = match cpu.try_reserve() {
                    Ok(permit) => permit,
                    Err(CpuError::AtCapacity { .. }) => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                (Some(permit), service.request(&source_key)?)
            };
        let bank = PopulationBank {
            catalog,
            catalogs,
            level: request.level,
            cache: self
                .cache
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?,
        };
        let prepare = Box::new(prepare);
        let task = match model {
            M2Load::Pending(demand) => {
                drop(permit);
                let mut bank = Some(bank);
                match PendingTask::dependent(cpu, &mut bank, demand, prepare) {
                    Ok(task) => task,
                    Err(error) => {
                        self.cache = Some(
                            bank.take()
                                .unwrap_or_else(|| {
                                    unreachable!("refused appearance retains its bank")
                                })
                                .cache,
                        );
                        return match error {
                            CpuError::AtCapacity { .. } => Ok(()),
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
                    .submit(move || bank.prepare(model, prepare));
                if let Some(demand) = &demand {
                    assert!(
                        demand.bind_service(task.service_control()),
                        "one producer binds each source task"
                    );
                }
                PendingTask::Direct { task, demand }
            }
        };
        self.pending = Some(PendingPopulation {
            identity: request.identity,
            key: request.key,
            level: request.level,
            withdrawn: false,
            stage: PopulationStage::Preparing(task),
        });
        Ok(())
    }
}
