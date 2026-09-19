//! Retained numeric continuation notices; domain state stays with the coordinator.

mod registration;
mod state;

use super::CoordinatorNotifier;
use crate::{CpuError, CpuStorageBudget, CpuStorageClass};
use state::Core;
use std::{marker::PhantomData, rc::Rc, sync::Arc};

/// A prerequisite outcome authorizes a coordinator to consume its own operation.
/// No Lua, renderer or other domain payload crosses this queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadyContinuation {
    pub(super) key: u64,
    pub(super) outcome: crate::JobOutcome,
}

impl ReadyContinuation {
    /// Returns the caller's operation identity within the current queue epoch.
    pub const fn key(self) -> u64 {
        self.key
    }

    /// A failed prerequisite requires domain cleanup rather than execution.
    pub const fn outcome(self) -> crate::JobOutcome {
        self.outcome
    }
}

/// Main-affinity readiness inbox with bounded, reusable node and edge storage.
///
/// Register a complete phase before moving its non-Send inputs. Taking a notice
/// transfers no domain state: the coordinator still owns execution, publication
/// and any completion producer needed by downstream workers. Cancellation only
/// removes this inbox's subscriptions, never the shared resource producer.
pub struct MainReadyQueue {
    core: Arc<Core>,
    bindings: crate::storage::StorageVec<super::readiness::Binding>,
    _main: PhantomData<Rc<()>>,
}

impl MainReadyQueue {
    /// Supplies the executor's existing durable coordinator signal.
    pub(crate) fn new(notifier: Option<Arc<dyn CoordinatorNotifier>>) -> Self {
        Self {
            core: Arc::new(Core::new(notifier)),
            bindings: crate::storage::StorageVec::new(),
            _main: PhantomData,
        }
    }

    /// Reserves the entire continuation phase before registration/input transfer.
    /// Storage remains charged and reusable across epochs, including cancellation.
    /// # Errors
    /// Rejects an undrained epoch, exhausted generation, allocation or byte budget.
    pub fn begin(
        &mut self,
        nodes: usize,
        inputs: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<(), CpuError> {
        let mut state = self.core.lock();
        if state.active && state.delivered != state.nodes.len() {
            return Err(CpuError::BatchActive);
        }
        let epoch = state.epoch.checked_add(1).ok_or(CpuError::EpochExhausted)?;
        state.reserve(nodes, inputs, budget, class)?;
        self.bindings
            .reserve(budget, class, crate::CpuStorageKind::Metadata, inputs)?;
        state.reset();
        state.epoch = epoch;
        state.active = true;
        state.node_limit = nodes;
        state.input_limit = inputs;
        Ok(())
    }

    /// Takes each ready notice once. Resource callbacks never invoke this consumer.
    pub fn take_ready(&mut self) -> Option<ReadyContinuation> {
        let mut state = self.core.lock();
        let index = state.ready.pop_front()?;
        let node = &mut state.nodes[index];
        let trace = node.trace;
        let notice = ReadyContinuation {
            key: node.key,
            outcome: node.outcome,
        };
        node.status = state::Status::Delivered;
        let inputs = node.inputs.clone();
        for input in &mut state.inputs[inputs] {
            input.subscription = None;
        }
        state.delivered += 1;
        drop(state);
        trace.link("cpu.main.consume");
        Some(notice)
    }

    /// Durable predicate for the native arm/check/park protocol.
    pub fn has_ready(&self) -> bool {
        !self.core.lock().ready.is_empty()
    }

    /// Offline coordinators park on durable readiness. Live runtime uses the
    /// native bridge with `has_ready`, allowing SDL service at the same boundary.
    /// # Errors
    /// Rejects an inactive/fully consumed epoch or an unfinished worker-side wait.
    pub fn wait_until_ready(&self) -> Result<(), CpuError> {
        let mut state = self.core.lock();
        while state.ready.is_empty() {
            if !state.active {
                return Err(CpuError::BatchInactive);
            }
            if state.delivered == state.nodes.len() {
                return Err(CpuError::BatchOpen);
            }
            if crate::is_worker_thread() {
                return Err(CpuError::WorkerWait);
            }
            state = self
                .core
                .changed
                .wait(state)
                .unwrap_or_else(|_| unreachable!("continuation metadata cannot panic"));
        }
        Ok(())
    }

    /// Abandons only numeric notices; the caller must retire its owned domain state.
    /// Late notifications cannot affect the next epoch.
    pub fn cancel(&mut self) {
        self.core.lock().reset();
        self.bindings.clear();
    }
}

impl Drop for MainReadyQueue {
    fn drop(&mut self) {
        self.cancel();
    }
}
