//! Per-consumer bounded readiness bridges preserve shared source and demand ownership.

use super::{M2LoadError, M2LoadRequest, Outcome, Slot};
use crate::{DecodedM2Model, ResourceLease};
use solarity_cpu::{
    CpuError, CpuStorageBudget, CpuStorageClass, ProductOutcome, ProductPublisher, ReadyToken,
    SharedProduct,
};
use std::sync::{Arc, Mutex};

/// One graph edge owns one port slot; no global fan-out limit is guessed.
/// Keep this owner until the dependent phase is reclaimed. Dropping it cancels
/// only this edge, never the shared source producer or another consumer.
pub struct M2LoadDependency {
    _owner: Arc<DependencyOwner>,
    product: SharedProduct<ResourceLease<DecodedM2Model>, M2LoadError>,
    // This lease retains this consumer's source priority until derived work returns.
    _interest: solarity_cpu::CpuServiceInterest,
}

/// Publication pins port and producer together, preventing last-consumer drop
/// from cancelling the port while a producer is publishing successful readiness.
pub(super) struct DependencyOwner {
    producer: Mutex<Option<ProductPublisher<ResourceLease<DecodedM2Model>, M2LoadError>>>,
}
impl DependencyOwner {
    /// Detaches the single producer before signaling scheduler metadata.
    fn complete(&self, outcome: Outcome) {
        let producer = self
            .producer
            .lock()
            .unwrap_or_else(|_| unreachable!("dependency producer metadata cannot panic"))
            .take();
        if let Some(producer) = producer {
            producer.publish(outcome);
        }
    }
}
impl M2LoadRequest {
    /// Reserves one graph subscription before the dependent owner transfers inputs.
    /// Publication and registration cannot miss one another: registration locks
    /// listeners before observing the result; publication releases result first.
    /// # Errors
    /// Returns ordinary CPU metadata admission errors without changing the request.
    pub fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<M2LoadDependency, CpuError> {
        let (producer, product) = SharedProduct::new(1, budget, class)?;
        let owner = Arc::new(DependencyOwner {
            producer: Mutex::new(Some(producer)),
        });
        let mut listeners = self
            .slot
            .dependencies
            .lock()
            .unwrap_or_else(|_| unreachable!("model dependency metadata cannot panic"));
        let outcome = self.poll();
        if outcome.is_none() {
            listeners.retain(|listener| listener.strong_count() != 0);
            listeners.push(Arc::downgrade(&owner));
        }
        drop(listeners);
        if let Some(outcome) = outcome {
            owner.complete(outcome);
        }
        Ok(M2LoadDependency {
            _owner: owner,
            product,
            _interest: self.interest.clone(),
        })
    }
}
impl M2LoadDependency {
    /// Borrows the one reserved consumer's readiness identity.
    #[must_use]
    pub fn readiness(&self) -> ReadyToken {
        self.product.readiness()
    }

    /// Observes the original source result; scheduler failure does not erase it.
    #[must_use]
    pub fn poll(&self) -> Option<Outcome> {
        self.product.poll().map(|outcome| match outcome {
            ProductOutcome::Succeeded(model) => Ok(model.clone()),
            ProductOutcome::Failed(error) => Err(error.clone()),
            ProductOutcome::Abandoned => Err(M2LoadError::Abandoned),
        })
    }
}
impl Slot {
    /// Dispatches readiness after durable source publication, outside source locks.
    pub(super) fn publish_dependencies(&self) {
        let outcome = {
            let result = self
                .result
                .lock()
                .unwrap_or_else(|_| unreachable!("model result metadata cannot panic"));
            result
                .as_ref()
                .unwrap_or_else(|| unreachable!("dependencies follow source publication"))
                .clone()
        };
        let listeners = std::mem::take(
            &mut *self
                .dependencies
                .lock()
                .unwrap_or_else(|_| unreachable!("model dependency metadata cannot panic")),
        );
        for listener in listeners {
            if let Some(owner) = listener.upgrade() {
                owner.complete(outcome.clone());
            }
        }
    }
}
