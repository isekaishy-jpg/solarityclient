//! Consumers poll independently; only explicit coordinator boundaries may wait.

use super::{M2LoadRequest, Outcome, Slot};
use solarity_cpu::{CpuService, CpuServiceControl};
use std::sync::Arc;

impl M2LoadRequest {
    /// Registers one consumer before returning its independently cloneable request handle.
    pub(super) fn new(slot: Arc<Slot>, service: CpuService) -> Result<Self, crate::AssetError> {
        let interest = slot.subscribe(service)?;
        Ok(Self { slot, interest })
    }

    /// Changes this consumer's CPU demand without overriding any other registered owner.
    pub fn set_service(&self, service: CpuService) {
        self.interest.set_service(service);
    }

    /// The producer owner binds its admitted task once; consumers never share task results.
    /// Returns false if a task was already bound to this source request.
    #[must_use]
    pub fn bind_service(&self, control: CpuServiceControl) -> bool {
        self.slot.demand.bind(control)
    }

    /// Returns durable completion without waiting or reviving scene membership.
    #[must_use]
    pub fn poll(&self) -> Option<Outcome> {
        self.slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("model result metadata cannot panic"))
            .clone()
            .map(|outcome| self.admit_result(outcome))
    }

    pub(super) fn admit_result(&self, outcome: Outcome) -> Outcome {
        let model = outcome?;
        if self.interest.service() != CpuService::Speculative {
            model
                .require_storage()
                .map_err(|error| super::M2LoadError::Asset(Arc::new(error)))?;
        }
        Ok(model)
    }

    /// Waits at an explicit coordinator/tooling boundary; CPU workers may consume only ready results.
    /// Normal frame code uses `poll` or `dependency`; an unfinished resource never parks a worker.
    /// # Errors
    /// Returns a source/producer failure, or rejects an unfinished wait on a CPU worker.
    pub fn wait(&self) -> Outcome {
        let mut result = self
            .slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("model result metadata cannot panic"));
        while result.is_none() {
            if solarity_cpu::is_worker_thread() {
                return Err(super::M2LoadError::WorkerWait);
            }
            result = self
                .slot
                .ready
                .wait(result)
                .unwrap_or_else(|_| unreachable!("model result metadata cannot panic"));
        }
        let outcome = result
            .as_ref()
            .unwrap_or_else(|| unreachable!("observed result remains published"))
            .clone();
        drop(result);
        self.admit_result(outcome)
    }
}
