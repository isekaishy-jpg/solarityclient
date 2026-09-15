//! Worker kernels release eligible successors before notifying the coordinator.

use super::JobOutcome;
use super::state::{Core, Status};
use crate::pool::dispatch::ReadyWork;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

impl<T: Send + 'static> ReadyWork for Core<T> {
    fn run(self: Arc<Self>) {
        loop {
            let (index, mut job, kernel, trace) = {
                let mut state = self.lock();
                let index = loop {
                    let Some(index) = state.ready.pop_front() else {
                        state.runners -= 1;
                        state.release_if_terminal();
                        self.ready.notify_all();
                        return;
                    };
                    if matches!(state.nodes[index].status, Status::Ready) {
                        break index;
                    }
                };
                state.nodes[index].status = Status::Running;
                (
                    index,
                    state.jobs[index]
                        .take()
                        .unwrap_or_else(|| unreachable!("ready job owns state")),
                    state.kernel,
                    state.trace,
                )
            };
            let _trace = trace.enter();
            let outcome = {
                let _execution = solarity_profiling::profile!("cpu.frame.execute");
                catch_unwind(AssertUnwindSafe(|| kernel.run(&mut job)))
                    .unwrap_or(JobOutcome::Panicked)
            };
            let mut state = self.lock();
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
        }
    }
}
