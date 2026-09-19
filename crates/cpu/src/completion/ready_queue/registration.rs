//! Transactional subscription registration precedes numeric readiness delivery.

use super::{
    MainReadyQueue,
    state::{Input, Node, Status},
};
use crate::completion::{ReadySink, ReadyToken};
use crate::{CpuError, JobOutcome};
use std::sync::Arc;

impl MainReadyQueue {
    /// Registers one main-only operation after all prerequisites. Failure makes
    /// its cleanup notice ready without waiting for remaining prerequisites.
    /// The caller retains all non-Send inputs until this registration succeeds.
    /// # Errors
    /// Reports inactive/full metadata, stale prerequisites or subscription limits;
    /// refusal removes every registration made by this call.
    pub fn watch(&mut self, key: u64, prerequisites: &[ReadyToken]) -> Result<(), CpuError> {
        let parent = solarity_profiling::TraceContext::capture();
        let mut state = self.core.lock();
        if !state.active {
            return Err(CpuError::BatchInactive);
        }
        if state.nodes.len() == state.node_limit
            || prerequisites.len() > state.input_limit - state.inputs.len()
        {
            return Err(CpuError::BatchCapacity);
        }
        let start = state.inputs.len();
        let index = state.nodes.len();
        for prerequisite in prerequisites {
            let subscription = match prerequisite.reserve() {
                Ok(subscription) => subscription,
                Err(error) => {
                    state
                        .inputs
                        .resize_with(start, || unreachable!("rollback only shrinks inputs"));
                    self.bindings.clear();
                    return Err(error);
                }
            };
            self.bindings.push(subscription.binder());
            state.inputs.push(Input {
                node: index,
                delivered: false,
                subscription: Some(subscription),
            });
        }
        let end = state.inputs.len();
        let trace = parent.fork("cpu.main.request");
        state.nodes.push(Node {
            trace,
            key,
            inputs: start..end,
            remaining: prerequisites.len(),
            status: Status::Waiting,
            outcome: JobOutcome::Succeeded,
        });
        let epoch = state.epoch;
        if prerequisites.is_empty() {
            state.ready(index, JobOutcome::Succeeded);
        }
        drop(state);
        let sink: Arc<dyn ReadySink> = self.core.clone();
        for (offset, binding) in self.bindings.drain(..).enumerate() {
            binding.bind(Arc::downgrade(&sink), epoch, start + offset);
        }
        // A main consumer declares a real dependency on this phase. Promotion
        // cannot alter execution eligibility or run gameplay callbacks.
        for prerequisite in prerequisites {
            prerequisite.require_urgent();
        }
        if prerequisites.is_empty() {
            trace.link("cpu.main.ready");
            self.core.notify();
        }
        Ok(())
    }
}
