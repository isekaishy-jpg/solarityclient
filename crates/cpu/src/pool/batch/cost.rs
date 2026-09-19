//! Phase cost publication never nests a dispatcher lock inside domain metadata.

use super::state::{Core, State};
use crate::pool::dispatch::ReadyWork;
use std::sync::{Arc, atomic::Ordering};

impl<T> Core<T> {
    /// Publishes after ready-list mutation. Decreases are repaired lazily by queue
    /// selection; increases refresh already admitted runners after this lock drops.
    pub(super) fn update_cost(&self, state: &mut State<T>) -> bool {
        let cost = state.ready_cost();
        // Every writer holds this phase's metadata guard. Avoid an exclusive
        // cache-line write for the common case of another job in the same bin.
        let previous = self.cost.load(Ordering::Relaxed);
        if previous != cost {
            self.cost.store(cost, Ordering::Release);
        }
        cost > previous && state.runners != 0 && state.service.is_none()
    }
}

impl<T: Send + 'static> Core<T> {
    /// Reads only the current dispatcher owner. Reuse cannot apply a stale cost:
    /// queue classification always reads this owner's latest atomic hint.
    pub(super) fn refresh_cost(self: &Arc<Self>) {
        let dispatch = self.lock().dispatch.clone();
        if let Some(dispatch) = dispatch {
            let work: Arc<dyn ReadyWork> = self.clone();
            dispatch.reclassify_cost(&work);
        }
    }
}
