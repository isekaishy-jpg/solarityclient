//! Per-consumer bounded readiness bridges preserve shared source and demand ownership.

use super::{M2LoadRequest, Outcome, Slot};
use solarity_cpu::{
    CompletionPort, CompletionProducer, CpuError, CpuStorageBudget, CpuStorageClass, JobOutcome,
    ReadyToken,
};
use std::sync::{Arc, Mutex};

/// One graph edge owns one port slot; no global fan-out limit is guessed.
/// Keep this owner until the dependent phase is reclaimed. Dropping it cancels
/// only this edge, never the shared source producer or another consumer.
pub struct M2LoadDependency {
    owner: Arc<DependencyOwner>,
    request: M2LoadRequest,
}

/// Publication pins port and producer together, preventing last-consumer drop
/// from cancelling the port while a producer is publishing successful readiness.
pub(super) struct DependencyOwner {
    port: CompletionPort,
    producer: Mutex<Option<CompletionProducer>>,
}
impl DependencyOwner {
    /// Detaches the single producer before signaling scheduler metadata.
    fn complete(&self, outcome: JobOutcome) {
        let producer = self
            .producer
            .lock()
            .unwrap_or_else(|_| unreachable!("dependency producer metadata cannot panic"))
            .take();
        if let Some(mut producer) = producer {
            let published = producer.complete(outcome);
            debug_assert!(
                published.is_ok(),
                "owned model dependency remains in its original generation"
            );
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
        let port = CompletionPort::new(1, budget, class)?;
        let producer = port.producer()?;
        let owner = Arc::new(DependencyOwner {
            port,
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
            owner.complete(job_outcome(&outcome));
        }
        Ok(M2LoadDependency {
            owner,
            request: self.clone(),
        })
    }
}
impl M2LoadDependency {
    /// Borrows the one reserved consumer's readiness identity.
    #[must_use]
    pub fn readiness(&self) -> ReadyToken {
        self.owner.port.readiness()
    }

    /// Observes the original source result; scheduler failure does not erase it.
    #[must_use]
    pub fn poll(&self) -> Option<Outcome> {
        self.request.poll()
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
            job_outcome(
                result
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("dependencies follow source publication")),
            )
        };
        let listeners = std::mem::take(
            &mut *self
                .dependencies
                .lock()
                .unwrap_or_else(|_| unreachable!("model dependency metadata cannot panic")),
        );
        for listener in listeners {
            if let Some(owner) = listener.upgrade() {
                owner.complete(outcome);
            }
        }
    }
}
/// Source error details remain in the request; only readiness crosses the CPU API.
fn job_outcome(result: &Outcome) -> JobOutcome {
    if result.is_ok() {
        JobOutcome::Succeeded
    } else {
        JobOutcome::Failed
    }
}
