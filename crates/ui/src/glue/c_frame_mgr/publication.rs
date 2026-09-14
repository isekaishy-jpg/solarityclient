//! Ordered entry callbacks with one cooperatively published native presentation.

use std::{ops::ControlFlow, task::Poll, time::Duration};

use super::FrameManager;
use crate::UiEventError;
use crate::startup::StartupTask;

/// Owns a covered event sequence and its unpublished frame until completion.
/// Cancellation drops the frame and restores its native sound/presentation scopes.
pub struct FramePublication<T>(StartupTask<(FrameManager, T), UiEventError>);

impl<T> FramePublication<T> {
    /// Advances between callback steps and publication operations. Individual
    /// authored callbacks remain indivisible and can exceed the requested budget.
    ///
    /// # Errors
    /// Returns a native presentation error after ordered callbacks have run.
    ///
    /// # Panics
    /// Panics if called after a ready result has already been returned.
    pub fn advance(&mut self, budget: Duration) -> Poll<Result<(FrameManager, T), UiEventError>> {
        self.0.advance(budget)
    }
}

impl FrameManager {
    /// Owns stock world-entry's sound-suppressed, atomic event sequence.
    /// Each callback step returns Continue until the sequence returns its result.
    /// Session publication and external UI dispatch must remain held until ready;
    /// the caller may present the loading card between advances.
    pub fn begin_suppressed_publication<T: 'static>(
        mut self,
        mut publish: impl FnMut(&mut Self) -> ControlFlow<T> + 'static,
    ) -> FramePublication<T> {
        FramePublication(StartupTask::new(move |budget| async move {
            let _sound = self.owner.suppress_sound_entries();
            let hold = self.owner.hold_presentation();
            let result = loop {
                match publish(&mut self) {
                    ControlFlow::Break(result) => break result,
                    ControlFlow::Continue(()) => budget.checkpoint().await,
                }
            };
            drop(hold);
            self.owner
                .flush_deferred_presentation_cooperatively(Some(&budget))
                .await?;
            Ok((self, result))
        }))
    }
}
