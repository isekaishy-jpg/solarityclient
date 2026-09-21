//! M2 consumers retain exact source results through the common budgeted readiness bridge.
use super::{M2LoadError, M2LoadRequest};
use crate::{DecodedM2Model, ResourceLease};
use solarity_cpu::{CpuError, CpuStorageBudget, CpuStorageClass};

/// One consumer's typed source result, readiness generation and live demand lease.
pub struct M2LoadDependency {
    source: super::super::super::source_dependency::SourceDependency<
        ResourceLease<DecodedM2Model>,
        M2LoadError,
    >,
    request: M2LoadRequest,
}
impl M2LoadDependency {
    /// The source's durable terminal readiness; byte promotion happens at consumption.
    #[must_use]
    pub fn readiness(&self) -> solarity_cpu::ReadyToken {
        self.source.readiness()
    }
    /// Links the suspended consumer's live priority to the original producer.
    /// # Errors
    /// Reports stale readiness or exhausted dependency capacity.
    pub fn task_dependency(&self) -> Result<solarity_cpu::CpuTaskDependency, CpuError> {
        self.source.task_dependency()
    }
    /// Required consumers atomically promote a speculative generation before receiving it.
    #[must_use]
    pub fn poll(&self) -> Option<Result<ResourceLease<DecodedM2Model>, M2LoadError>> {
        self.source
            .poll()
            .map(|outcome| self.request.admit_result(outcome))
    }
}

impl M2LoadRequest {
    /// Reserve the consumer's edge before transferring owned work to a dependency phase.
    /// # Errors
    /// Returns ordinary CPU metadata admission errors without changing the request.
    pub fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<M2LoadDependency, CpuError> {
        Ok(M2LoadDependency {
            source: self
                .slot
                .dependency(budget, class, self.interest.clone(), || {
                    M2LoadError::Abandoned
                })?,
            request: self.clone(),
        })
    }
}
