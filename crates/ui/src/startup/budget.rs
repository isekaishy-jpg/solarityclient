//! Cooperative deadlines between indivisible native or authored operations.

use std::{cell::Cell, future::poll_fn, rc::Rc, task::Poll, time::Instant};

/// Shared deadline for one caller-owned construction poll. No deadline means
/// synchronous completion through the identical construction path.
#[derive(Clone)]
pub(crate) struct StartupBudget(pub(super) Rc<Cell<Option<Instant>>>);

impl StartupBudget {
    /// Suspends once after the current operation has exhausted its frame budget.
    /// The caller explicitly polls next frame; this is not an executor wakeup.
    pub(crate) async fn checkpoint(&self) {
        if self
            .0
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            let mut yielded = false;
            poll_fn(|_| {
                if std::mem::replace(&mut yielded, true) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
    }
}
