//! M2 consumers retain exact source results through the common budgeted readiness bridge.
use super::{M2LoadError, M2LoadRequest};
use crate::{DecodedM2Model, ResourceLease};
use solarity_cpu::{CpuError, CpuStorageBudget, CpuStorageClass};

/// One consumer's typed source result, readiness generation and live demand lease.
pub type M2LoadDependency = super::super::super::source_dependency::SourceDependency<
    ResourceLease<DecodedM2Model>,
    M2LoadError,
>;

impl M2LoadRequest {
    /// Reserve the consumer's edge before transferring owned work to a dependency phase.
    /// # Errors
    /// Returns ordinary CPU metadata admission errors without changing the request.
    pub fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<M2LoadDependency, CpuError> {
        self.slot
            .dependency(budget, class, self.interest.clone(), || {
                M2LoadError::Abandoned
            })
    }
}
