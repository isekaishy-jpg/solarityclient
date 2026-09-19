//! Population primary-model admission joins the namespace's existing decode authority.

use super::super::worker_presentation::{AppearanceRequest, AppearanceTask};
use super::super::{RuntimePlayerError, RuntimePlayerPresentation, RuntimePlayerSharedCatalogs};
use super::{
    PendingPopulation, PopulationRequest, PopulationStage, PopulationWorker, PreparedPopulation,
};
use solarity_asset::{ArchiveCatalog, DecodedM2Model, ResourceLease};
use solarity_cpu::CpuExecutor;

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
        let Some(task) = AppearanceTask::submit(
            cpu,
            AppearanceRequest {
                catalog,
                catalogs,
                level: request.level,
                model_path: request.model_path,
            },
            &mut self.cache,
            prepare,
        )?
        else {
            return Ok(());
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
