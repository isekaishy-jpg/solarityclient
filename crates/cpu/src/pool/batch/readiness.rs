//! External phase gates and executor-owned shutdown; no worker waits on a gate.

use super::state::{Core, Gate, Status};
use crate::JobOutcome;
use crate::completion::ReadySink;
use crate::pool::epochs::EpochOwner;
use std::sync::Arc;

impl<T: Send + 'static> ReadySink for Core<T> {
    fn signal(self: Arc<Self>, epoch: u64, input: usize, outcome: JobOutcome) {
        let mut state = self.lock();
        if state.generation != epoch || !matches!(state.gate, Gate::Pending(_)) {
            return;
        }
        let Some(subscription) = state.subscriptions.get_mut(input).and_then(Option::take) else {
            return;
        };
        drop(subscription);
        if outcome == JobOutcome::Succeeded {
            if let Gate::Pending(remaining) = state.gate {
                state.gate = if remaining == 1 {
                    Gate::Ready
                } else {
                    Gate::Pending(remaining - 1)
                };
            }
        } else {
            state.fail_gate();
        }
        let mut launch = state.runners_to_launch();
        // Even an empty/failed closed phase publishes from a scheduled runner.
        // Chained external failures cannot recurse through an unbounded call stack.
        if launch == 0 && state.can_finish() {
            state.runners = 1;
            launch = 1;
        }
        let notifier = state.notifier.clone();
        drop(state);
        self.launch(launch);
        self.ready.notify_all();
        if let Some(notifier) = notifier {
            notifier.notify();
        }
    }
}

impl<T: Send + 'static> EpochOwner for Core<T> {
    fn live(&self, epoch: u64) -> bool {
        let state = self.lock();
        state.generation == epoch && state.lease.is_some()
    }
    fn stop(self: Arc<Self>, epoch: u64) {
        let mut state = self.lock();
        if state.generation != epoch {
            return;
        }
        state.open = false;
        if matches!(state.gate, Gate::Pending(_)) {
            state.fail_gate();
        }
        let launch = state.runners_to_launch();
        drop(state);
        self.launch(launch);
        self.finish_if_terminal();
    }
}

impl<T: Send + 'static> Core<T> {
    /// The last runner/coordinator publishes phase completion before releasing
    /// admission. Reclamation cannot recycle a port whose delivery is still active.
    pub(super) fn finish_if_terminal(&self) {
        let mut state = self.lock();
        if !state.can_finish() {
            return;
        }
        state.finishing = true;
        let outcome = state
            .nodes
            .iter()
            .find_map(|node| match node.status {
                Status::Terminal(outcome) if outcome != JobOutcome::Succeeded => Some(outcome),
                _ => None,
            })
            .unwrap_or(if matches!(state.gate, Gate::Failed) {
                JobOutcome::DependencyFailed
            } else {
                JobOutcome::Succeeded
            });
        let completion = state
            .completion
            .clone()
            .unwrap_or_else(|| unreachable!("admitted phase owns completion identity"));
        drop(state);
        self.completion_port.complete(&completion, outcome);
        let mut state = self.lock();
        state.finishing = false;
        state.lease = None;
        let notifier = state.notifier.clone();
        drop(state);
        self.ready.notify_all();
        if let Some(notifier) = notifier {
            notifier.notify();
        }
    }
}
