//! Structured ownership and poll-only timing for local construction.

use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use super::StartupBudget;
use crate::GlueError;

/// Owns every partially constructed object and releases it on cancellation.
/// Its only suspension points are the budget checkpoints above.
pub(crate) struct StartupTask<T, E = GlueError> {
    future: Option<ConstructionFuture<T, E>>,
    budget: StartupBudget,
}

/// Erases the private local construction graph while preserving its error type.
type ConstructionFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;

impl<T, E> StartupTask<T, E> {
    /// Pins one local construction graph without exposing borrowed source plans.
    pub(crate) fn new<F>(construct: impl FnOnce(StartupBudget) -> F) -> Self
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let budget = StartupBudget(Rc::new(Cell::new(None)));
        Self {
            future: Some(Box::pin(construct(budget.clone()))),
            budget,
        }
    }

    /// Charges only the work executed in this poll, excluding inter-frame waits.
    /// A completed task must be consumed, never advanced again.
    pub(crate) fn advance(&mut self, budget: Duration) -> Poll<Result<T, E>> {
        self.budget.0.set(Instant::now().checked_add(budget));
        self.poll()
    }

    /// Completes the same graph synchronously for Glue and diagnostic callers.
    pub(crate) fn complete(mut self) -> Result<T, E> {
        self.budget.0.set(None);
        match self.poll() {
            Poll::Ready(result) => result,
            // Only budget checkpoints may suspend this private construction task.
            Poll::Pending => unreachable!("unbudgeted UI construction cannot suspend"),
        }
    }

    /// Drops the future's temporary plans and guards immediately on completion.
    fn poll(&mut self) -> Poll<Result<T, E>> {
        let _profile = solarity_profiling::profile!("ui.startup.slice");
        let Some(future) = self.future.as_mut() else {
            unreachable!("completed UI construction must not be polled again");
        };
        let result = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        if result.is_ready() {
            self.future = None;
        }
        result
    }
}

/// Completes a graph whose checkpoint budget is explicitly disabled. This also
/// accepts borrowed futures, so ordinary live snapshots need no task allocation.
pub(crate) fn complete_unyielding<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("unbudgeted UI construction cannot suspend"),
    }
}
