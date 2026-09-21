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
    listeners: Mutex<crate::AssetStorageVec<Listener<T, E>>>,
    budget: Option<CpuStorageBudget>,
    _memory: Option<solarity_cpu::ByteReservation>,
}

impl<T, E> SourceSlot<T, E> {
    pub(super) fn new(budget: Option<&CpuStorageBudget>) -> Result<Arc<Self>, CpuError> {
        let memory = budget
            .map(|budget| {
                budget.reserve(
                    CpuStorageClass::Required,
                    solarity_cpu::CpuStorageKind::Metadata,
                    size_of::<Self>() + 2 * size_of::<usize>(),
                )
            })
            .transpose()?;
        Ok(Arc::new(Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
            demand: budget
                .map(CpuServiceDemand::admitted)
                .transpose()?
                .unwrap_or_default(),
            listeners: Mutex::new(crate::AssetStorageVec::metadata()),
            budget: budget.cloned(),
            _memory: memory,
        }))
    }

    pub(super) fn subscribe(
        &self,
        service: solarity_cpu::CpuService,
    ) -> Result<CpuServiceInterest, crate::AssetError> {
        match &self.budget {
            Some(budget) => self
                .demand
                .subscribe_admitted(service, budget)
                .map_err(Into::into),
            None => Ok(self.demand.subscribe(service)),
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
    _memory: Arc<solarity_cpu::ByteReservation>,
}

/// Weak registration also pins its allocation charge until the weak Arc retires.
struct Listener<T, E> {
    owner: Weak<DependencyOwner<T, E>>,
    _memory: Arc<solarity_cpu::ByteReservation>,
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
        let memory = Arc::new(budget.reserve(
            class,
            solarity_cpu::CpuStorageKind::Metadata,
            size_of::<DependencyOwner<T, E>>()
                + size_of::<solarity_cpu::ByteReservation>()
                + 4 * size_of::<usize>(),
        )?);
        let owner = Arc::new(DependencyOwner {
            producer: Mutex::new(Some(producer)),
            _memory: Arc::clone(&memory),
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
            listeners.retain(|listener| listener.owner.strong_count() != 0);
            let policy = crate::AssetReadBudget::for_class(budget.clone(), class);
            listeners
                .push(
                    Some(&policy),
                    Listener {
                        owner: Arc::downgrade(&owner),
                        _memory: memory,
                    },
                )
                .map_err(|error| match error {
                    crate::AssetError::SourceStorage(error) => error,
                    _ => unreachable!("typed storage only reports CPU admission"),
                })?;
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
        let mut listeners = std::mem::take(
            &mut *self
                .listeners
                .lock()
                .unwrap_or_else(|_| unreachable!("source dependency metadata cannot panic")),
        );
        for listener in listeners.drain() {
            if let Some(owner) = listener.owner.upgrade() {
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

#[cfg(test)]
#[path = "../../tests/cache/source_dependency.rs"]
mod tests;
