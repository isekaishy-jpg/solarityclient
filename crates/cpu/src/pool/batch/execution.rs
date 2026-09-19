//! Worker kernels release eligible successors before notifying the coordinator.

use super::JobOutcome;
use super::state::{Core, Status};
use crate::pool::dispatch::ReadyWork;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl<T: Send + 'static> ReadyWork for Core<T> {
    fn urgent(&self) -> bool {
        self.urgent.load(Ordering::Acquire)
    }

    fn propagate(&self, epoch: u64) {
        self.propagate_priority(epoch);
    }

    fn run(self: Arc<Self>, flexible: bool) {
        let (dispatch, loading) = {
            let state = self.lock();
            (
                state
                    .dispatch
                    .clone()
                    .unwrap_or_else(|| unreachable!("admitted runner owns dispatch")),
                state.service.is_some(),
            )
        };
        loop {
            let (index, mut job, kernel, trace, cost) = {
                let mut state = self.lock();
                let index = loop {
                    let Some(index) = state.pop_ready() else {
                        state.runners -= 1;
                        drop(state);
                        self.finish_if_terminal();
                        self.ready.notify_all();
                        return;
                    };
                    if matches!(state.nodes[index].status, Status::Ready) {
                        break index;
                    }
                };
                state.nodes[index].status = Status::Running;
                let trace = state.trace;
                state.drain_tail.dispatch(trace);
                (
                    index,
                    state.jobs[index]
                        .take()
                        .unwrap_or_else(|| unreachable!("ready job owns state")),
                    state.kernel,
                    state.trace,
                    state.nodes[index].cost,
                )
            };
            let _trace = trace.enter();
            let outcome = {
                let mut execution = if loading {
                    solarity_profiling::profile!("cpu.load.execute")
                } else {
                    solarity_profiling::profile!("cpu.frame.execute")
                };
                execution.trace_owner(
                    index as u64 + 1,
                    cost.duration()
                        .map_or(0, |duration| duration.as_nanos() as u64),
                );
                catch_unwind(AssertUnwindSafe(|| kernel.run(&mut job)))
                    .unwrap_or(JobOutcome::Panicked)
            };
            let mut state = self.lock();
            state.drain_tail.returned(index);
            state.jobs[index] = Some(job);
            let outcome = if state.nodes[index].cancel_requested && outcome != JobOutcome::Panicked
            {
                JobOutcome::Cancelled
            } else {
                outcome
            };
            state.complete(index, outcome);
            let launch = state.runners_to_launch();
            let notifier = state.notifier.clone();
            drop(state);
            self.launch(launch);
            self.ready.notify_all();
            if let Some(notifier) = notifier {
                notifier.notify();
            }
            if loading || dispatch.should_yield(self.urgent(), flexible) {
                // Keep this runner reservation live while handing its lane back.
                // No borrowed input or user operation crosses the queue boundary.
                self.launch(1);
                return;
            }
        }
    }
}
