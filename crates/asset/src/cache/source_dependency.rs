//! Typed resource publication bridges discovered source dependencies to owned CPU services.

use solarity_cpu::{
    CpuError, CpuServiceDemand, CpuServiceInterest, CpuStorageBudget, CpuStorageClass,
    CpuTaskDependency, ProductOutcome, ProductPublisher, ReadyToken, SharedProduct,
};
use std::sync::{Arc, Condvar, Mutex, Weak};

/// Only small request metadata is synchronized; decoders and resource destructors stay outside.
pub(super) struct SourceSlot<T, E> {
    pub(super) result: Mutex<Option<Result<T, E>>>,
    pub(super) ready: Condvar,
    pub(super) demand: CpuServiceDemand,
    listeners: Mutex<Vec<Weak<DependencyOwner<T, E>>>>,
}

impl<T, E> Default for SourceSlot<T, E> {
    fn default() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
            demand: CpuServiceDemand::default(),
            listeners: Mutex::new(Vec::new()),
        }
    }
}

/// A single consumer retains its own readiness port, typed outcome and live demand lease.
pub struct SourceDependency<T, E> {
    _owner: Arc<DependencyOwner<T, E>>,
    product: SharedProduct<T, E>,
    interest: CpuServiceInterest,
    abandoned: fn() -> E,
}

/// Registration pins the publisher until the domain has made a durable result available.
struct DependencyOwner<T, E> {
    producer: Mutex<Option<ProductPublisher<T, E>>>,
}

impl<T, E> DependencyOwner<T, E> {
    /// Detach before calling the CPU notifier; no resource metadata lock reaches dispatch.
    fn complete(&self, outcome: Result<T, E>) {
        let producer = self
            .producer
            .lock()
            .unwrap_or_else(|_| unreachable!("source publisher metadata cannot panic"))
            .take();
        if let Some(producer) = producer {
            producer.publish(outcome);
        }
    }
}

impl<T: Clone, E: Clone> SourceSlot<T, E> {
    /// Reserve before suspension. Registration and source publication cannot miss one another.
    pub(super) fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        interest: CpuServiceInterest,
        abandoned: fn() -> E,
    ) -> Result<SourceDependency<T, E>, CpuError> {
        let (producer, product) = SharedProduct::new(1, budget, class)?;
        let owner = Arc::new(DependencyOwner {
            producer: Mutex::new(Some(producer)),
        });
        let mut listeners = self
            .listeners
            .lock()
            .unwrap_or_else(|_| unreachable!("source dependency metadata cannot panic"));
        let outcome = self
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("source result metadata cannot panic"))
            .clone();
        if outcome.is_none() {
            listeners.retain(|listener| listener.strong_count() != 0);
            listeners.push(Arc::downgrade(&owner));
        }
        drop(listeners);
        if let Some(outcome) = outcome {
            owner.complete(outcome);
        }
        Ok(SourceDependency {
            _owner: owner,
            product,
            interest,
            abandoned,
        })
    }

    /// Called after result publication, outside the request table and decoder's source lock.
    pub(super) fn publish_dependencies(&self) {
        let outcome = self
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("source result metadata cannot panic"))
            .as_ref()
            .unwrap_or_else(|| unreachable!("dependencies follow source publication"))
            .clone();
        let listeners = std::mem::take(
            &mut *self
                .listeners
                .lock()
                .unwrap_or_else(|_| unreachable!("source dependency metadata cannot panic")),
        );
        for listener in listeners {
            if let Some(owner) = listener.upgrade() {
                owner.complete(outcome.clone());
            }
        }
    }
}

impl<T: Clone, E: Clone> SourceDependency<T, E> {
    /// Reserves the scheduler edge and links the suspended consumer's live priority to its producer.
    /// # Errors
    /// Rejects stale readiness or a second subscriber for this single-owner edge.
    pub fn task_dependency(&self) -> Result<CpuTaskDependency, CpuError> {
        Ok(CpuTaskDependency::new(&self.readiness())?.with_demand(self.interest.clone()))
    }

    /// Exposes only readiness; the domain retains the typed outcome until consumption.
    #[must_use]
    pub fn readiness(&self) -> ReadyToken {
        self.product.readiness()
    }

    /// Returns the original source failure independently of scheduler cancellation.
    #[must_use]
    pub fn poll(&self) -> Option<Result<T, E>> {
        self.product.poll().map(|outcome| match outcome {
            ProductOutcome::Succeeded(value) => Ok(value.clone()),
            ProductOutcome::Failed(error) => Err(error.clone()),
            ProductOutcome::Abandoned => Err((self.abandoned)()),
        })
    }
}
