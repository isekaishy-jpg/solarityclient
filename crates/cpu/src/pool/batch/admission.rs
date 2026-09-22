//! Transactional phase admission and multi-producer prerequisite subscription.

use super::state::Gate;
use super::{FrameBatch, FrameBatchPlan, FramePriority};
use crate::completion::PrioritySink;
use crate::{CpuError, CpuExecutor, CpuStorageReservation, CpuStorageWorkingSet, ReadyToken};
use std::sync::Arc;

impl<T: Send + 'static> FrameBatch<T> {
    /// Reserves all readiness edges and metadata before transferring any input.
    /// Binding occurs outside the batch lock because an already-ready source can
    /// synchronously deliver scheduler metadata. No domain operation runs there.
    pub(super) fn begin_dependencies(
        &mut self,
        cpu: &CpuExecutor,
        plan: FrameBatchPlan,
        dependencies: &[ReadyToken],
    ) -> Result<(), CpuError> {
        self.begin_connected(cpu, plan, dependencies, 0, true)
            .map(|_| ())
    }

    /// Opens incremental publication only after scheduler and domain storage fit together.
    /// Preparation runs outside scheduler locks, before any input can be published.
    /// The callback must allocate from the supplied fund; unused capacity is released
    /// before this method returns. Callers must close and reclaim a successful epoch.
    /// # Errors
    /// Capacity/readiness refusal does not invoke preparation. Preparation errors or
    /// unwinds retire the empty epoch and its subscriptions, retaining caller ownership.
    /// The callback is responsible for rollback of its own values.
    pub fn begin_with_storage(
        &mut self,
        cpu: &CpuExecutor,
        plan: FrameBatchPlan,
        dependencies: &[ReadyToken],
        domain_bytes: usize,
        prepare: impl FnOnce(&mut CpuStorageReservation) -> Result<(), CpuError>,
    ) -> Result<(), CpuError> {
        let mut reservation = self.begin_connected(cpu, plan, dependencies, domain_bytes, false)?;
        {
            let mut admission = EmptyBinding {
                batch: self,
                prepared: false,
            };
            prepare(&mut reservation)?;
            admission.prepared = true;
        }
        drop(reservation);
        if plan.priority == FramePriority::Prerequisite {
            let generation = self.core.lock().generation;
            self.core.clone().require_urgent(generation);
        }
        Ok(())
    }

    /// Domain bytes remain protected after scheduler allocation and until input binding.
    pub(super) fn begin_connected(
        &mut self,
        cpu: &CpuExecutor,
        plan: FrameBatchPlan,
        dependencies: &[ReadyToken],
        domain_bytes: usize,
        promote: bool,
    ) -> Result<CpuStorageReservation, CpuError> {
        if self.active {
            return Err(CpuError::BatchActive);
        }
        for (index, dependency) in dependencies.iter().enumerate() {
            if dependencies[..index]
                .iter()
                .any(|earlier| earlier.same_generation(dependency))
            {
                return Err(CpuError::DuplicateReadiness);
            }
        }
        let (lease, class, epochs) = match self.service {
            None => (
                cpu.frame_state.reserve()?,
                crate::CpuStorageClass::Frame,
                &cpu.epochs,
            ),
            Some(service) => {
                let lease = cpu.reserve_service(service)?;
                let class = if service == crate::CpuService::Speculative {
                    crate::CpuStorageClass::Speculative
                } else {
                    crate::CpuStorageClass::Required
                };
                (lease, class, &cpu.load_epochs)
            }
        };
        let service_identity = self
            .service
            .map(|service| {
                crate::pool::task::ServiceIdentity::reserve(&cpu.dispatch, service, cpu.storage())
            })
            .transpose()?;
        let mut state = self.core.lock();
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(CpuError::EpochExhausted)?;
        let mut working_set = CpuStorageWorkingSet::default();
        state.include_storage(
            plan,
            dependencies.len(),
            cpu.storage(),
            class,
            &mut working_set,
        )?;
        self.core.completion_port.include_storage(
            cpu.completion_capacity,
            cpu.storage(),
            class,
            &mut working_set,
        )?;
        working_set.include(domain_bytes, 0)?;
        let mut reservation = cpu
            .storage()
            .reserve_working_set(class, working_set.bytes())?;
        state.reserve_reserved(plan, dependencies.len(), &mut reservation)?;
        for dependency in dependencies {
            match dependency.reserve() {
                Ok(subscription) => state.subscriptions.push(Some(subscription)),
                Err(error) => {
                    state.subscriptions.clear();
                    return Err(error);
                }
            }
        }
        let completion = match self
            .core
            .completion_port
            .begin_reserved(cpu.completion_capacity, &mut reservation)
        {
            Ok(completion) => completion,
            Err(error) => {
                state.subscriptions.clear();
                return Err(error);
            }
        };
        state.generation = generation;
        state.dependencies.extend_from_slice(dependencies);
        self.core
            .urgent
            .store(false, std::sync::atomic::Ordering::Release);
        self.core
            .cost
            .store(0, std::sync::atomic::Ordering::Release);
        let priority_owner: Arc<dyn PrioritySink> = self.core.clone();
        self.core
            .completion_port
            .priority_owner(Arc::downgrade(&priority_owner), generation);
        state.plan = plan;
        state.open = true;
        state.gate = if dependencies.is_empty() {
            Gate::Ready
        } else {
            Gate::Pending(dependencies.len())
        };
        state.completion = Some(completion);
        // Independent resource kernels use the configured flexible capacity.
        // Dispatch separately enforces the bulk limit and keeps every loading
        // runner off protected workers, including after urgency propagation.
        state.workers = if self.service.is_some() {
            cpu.background_worker_count()
        } else {
            cpu.worker_count()
        };
        state.service = service_identity;
        state.dispatch = Some(Arc::clone(&cpu.dispatch));
        state.notifier = cpu.notifier.clone();
        state.trace = solarity_profiling::TraceContext::capture().fork(if self.service.is_some() {
            "cpu.load.request"
        } else {
            "cpu.frame.request"
        });
        state.lease = Some(lease);
        self.core
            .live_epoch
            .store(generation, std::sync::atomic::Ordering::Release);
        self.active = true;
        drop(state);
        let owner: Arc<dyn crate::pool::epochs::EpochOwner> = self.core.clone();
        if let Err(error) = epochs.register(
            Arc::downgrade(&owner),
            Arc::downgrade(&self.core.live_epoch),
            generation,
        ) {
            owner.stop(generation);
            self.core.finish_if_terminal();
            self.core.lock().clear();
            self.active = false;
            return Err(error);
        }
        let sink: Arc<dyn crate::completion::ReadySink> = self.core.clone();
        for input in 0..dependencies.len() {
            let binder = self
                .core
                .lock()
                .subscriptions
                .get(input)
                .and_then(Option::as_ref)
                .map(crate::completion::Subscription::binder);
            if let Some(binder) = binder {
                binder.bind(Arc::downgrade(&sink), generation, input);
            }
        }
        if promote && plan.priority == FramePriority::Prerequisite {
            self.core.clone().require_urgent(generation);
        }
        Ok(reservation)
    }
}

/// Preparation errors and unwinds cannot retain a worker lease or readiness edge.
/// No inputs have moved and no kernel can run while this guard is armed.
struct EmptyBinding<'a, T: Send + 'static> {
    batch: &'a mut FrameBatch<T>,
    prepared: bool,
}
impl<T: Send + 'static> Drop for EmptyBinding<'_, T> {
    fn drop(&mut self) {
        if self.prepared {
            return;
        }
        let epoch = self.batch.core.lock().generation;
        let owner: std::sync::Arc<dyn crate::pool::epochs::EpochOwner> = self.batch.core.clone();
        owner.stop(epoch);
        self.batch.core.finish_if_terminal();
        self.batch.core.lock().clear();
        self.batch.active = false;
    }
}
