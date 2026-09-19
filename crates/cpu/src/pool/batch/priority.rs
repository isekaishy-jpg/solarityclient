//! Promotion follows unresolved phase prerequisites without recursive callbacks.

use super::FrameBatch;
use super::state::{Core, Gate};
use crate::CpuError;
use crate::completion::PrioritySink;
use crate::pool::dispatch::{ReadyWork, Work, WorkClass};
use std::sync::{Arc, atomic::Ordering};

impl<T: Send + 'static> PrioritySink for Core<T> {
    fn require_urgent(self: Arc<Self>, epoch: u64) {
        let mut state = self.lock();
        if state.generation != epoch
            || state.lease.is_none()
            || state.finishing
            || self.urgent.load(Ordering::Relaxed)
        {
            return;
        }
        self.urgent.store(true, Ordering::Release);
        // This reservation pins the epoch until propagation finishes. Empty
        // phases cannot recycle under a queued promotion carrying their identity.
        state.priority_pending = true;
        let dispatch = state
            .dispatch
            .clone()
            .unwrap_or_else(|| unreachable!("admitted phase owns dispatch"));
        let service = state.service.clone();
        drop(state);
        let work: Arc<dyn ReadyWork> = self;
        if let Some(service) = service {
            dispatch.reclassify(&service, crate::CpuService::Required);
        } else {
            dispatch.promote(&work);
        }
        dispatch.push(Work::Priority(work, epoch), WorkClass::Priority);
    }
}

impl<T: Send + 'static> Core<T> {
    /// Visits each dependency once for this phase's monotonic promotion. Source
    /// callbacks enqueue their own metadata, so deep chains do not recurse.
    pub(super) fn propagate_priority(&self, epoch: u64) {
        let mut input = 0;
        loop {
            let dependency = {
                let state = self.lock();
                if state.generation != epoch || !state.priority_pending {
                    return;
                }
                if matches!(state.gate, Gate::Failed) {
                    None
                } else {
                    state.dependencies.get(input).cloned()
                }
            };
            let Some(dependency) = dependency else {
                break;
            };
            dependency.require_urgent();
            input += 1;
        }
        self.lock().priority_pending = false;
        self.finish_if_terminal();
    }
}

impl<T: Send + 'static> FrameBatch<T> {
    /// Promotes this phase and its unresolved prerequisites. Running kernels
    /// finish normally; queue urgency never changes their execution class.
    /// # Errors
    /// Reports an inactive phase without changing a later binding.
    pub fn require_urgent(&self) -> Result<(), CpuError> {
        if !self.active {
            return Err(CpuError::BatchInactive);
        }
        let epoch = self.core.lock().generation;
        self.core.clone().require_urgent(epoch);
        Ok(())
    }
}
