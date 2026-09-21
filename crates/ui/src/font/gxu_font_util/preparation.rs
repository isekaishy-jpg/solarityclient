//! Exclusive native UI loans return before main resumes Lua or publication.

use super::{FontSystem, Owner, work::Request};
use crate::FontError;
use solarity_asset::AssetStore;
use std::sync::{Arc, Mutex};

impl FontSystem {
    /// The runtime host joins on every outcome, including native failure/unwind.
    /// Consequently even partial state is reclaimed before this loan is released.
    pub(crate) fn prepare<T, R, F>(
        &self,
        assets: &mut AssetStore,
        state: &mut T,
        operation: F,
    ) -> Result<R, FontError>
    where
        T: Default + Send + 'static,
        R: Send + 'static,
        F: FnOnce(&mut T, &mut FontSystem, &mut AssetStore) -> R + Send + 'static,
    {
        let Owner::Worker(worker) = &self.owner else {
            return Ok(operation(state, &mut self.clone(), assets));
        };
        let loan = Loan {
            shared: Arc::new(Mutex::new((std::mem::take(state), None))),
            target: state,
        };
        let shared = Arc::clone(&loan.shared);
        worker.execute(
            assets.namespace(),
            Request::Preparation(Box::new(move |fonts, assets| {
                let mut state = shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let result = operation(&mut state.0, fonts, assets);
                state.1 = Some(result);
            })),
        )?;
        let result = loan
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1
            .take();
        result.ok_or_else(|| FontError::Execution {
            message: "UI preparation completed without its typed output".into(),
        })
    }
}

struct Loan<'a, T: Default, R> {
    shared: Arc<Mutex<(T, Option<R>)>>,
    target: &'a mut T,
}

impl<T: Default, R> Drop for Loan<'_, T, R> {
    fn drop(&mut self) {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::swap(self.target, &mut shared.0);
    }
}
