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

    fn cost(&self) -> u8 {
        self.cost.load(Ordering::Acquire)
    }

    fn propagate(&self, epoch: u64) {
        self.propagate_priority(epoch);
    }

    fn run(self: Arc<Self>, flexible: bool, worker: crate::pool::WorkerLane) {
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
            let (index, mut job, kernel, trace, cost, generation, cancellation) = {
                let mut state = self.lock();
                if !loading
                    && !state.ready.is_empty()
                    && dispatch.heavier_is_queued(self.urgent(), self.cost())
                {
                    let trace = state.trace;
                    drop(state);
                    trace.value("cpu.frame.cost_preempt", 0, 0, 1);
                    self.launch(1);
                    return;
                }
                let index = loop {
                    let Some(index) = state.pop_ready() else {
                        self.update_cost(&mut state);
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
                self.update_cost(&mut state);
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
                    state.generation,
                    state
                        .kernel
                        .uses_context()
                        .then(|| Arc::clone(&state.cancellation)),
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
                let context = cancellation.as_ref().map(|flags| {
                    crate::JobContext::new(generation, index, &flags[index], trace, worker)
                });
                catch_unwind(AssertUnwindSafe(|| kernel.run(&mut job, context.as_ref())))
                    .unwrap_or(JobOutcome::Panicked)
            };
            // No context/page pin survives terminal publication or next-epoch reserve.
            drop(cancellation);
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
            let raised = self.update_cost(&mut state);
            let has_ready = !state.ready.is_empty();
            let launch = state.runners_to_launch();
            let notifier = state.notifier.clone();
            drop(state);
            if raised {
                self.refresh_cost();
            }
            self.launch(launch);
            self.ready.notify_all();
            if let Some(notifier) = notifier {
                notifier.notify();
            }
            if has_ready && (loading || dispatch.should_yield(self.urgent(), flexible, self.cost()))
            {
                // Keep this runner reservation live while handing its lane back.
                // No borrowed input or user operation crosses the queue boundary.
                self.launch(1);
                return;
            }
        }
    }
}
