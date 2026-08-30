//! Worker admission, shutdown, and completion accounting.

use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::pool::{CpuError, CpuPoolSnapshot};

/// Mutable lifecycle fields protected as one admission/shutdown transaction.
struct Lifecycle {
    accepting: bool,
    in_flight: usize,
}

/// Shared state kept alive until the last admitted worker closure completes.
pub(crate) struct SharedExecutorState {
    lifecycle: Mutex<Lifecycle>,
    drained: Condvar,
    max_in_flight: NonZeroUsize,
}

impl SharedExecutorState {
    /// Creates the lifecycle in its accepting, empty state.
    pub(crate) fn new(max_in_flight: NonZeroUsize) -> Arc<Self> {
        Arc::new(Self {
            lifecycle: Mutex::new(Lifecycle {
                accepting: true,
                in_flight: 0,
            }),
            drained: Condvar::new(),
            max_in_flight,
        })
    }

    /// Atomically admits one task against both capacity and shutdown state.
    pub(crate) fn reserve(self: &Arc<Self>) -> Result<WorkerLease, CpuError> {
        let mut lifecycle = self.lock()?;
        if !lifecycle.accepting {
            return Err(CpuError::ShuttingDown);
        }
        if lifecycle.in_flight >= self.max_in_flight.get() {
            return Err(CpuError::AtCapacity {
                limit: self.max_in_flight,
            });
        }

        lifecycle.in_flight += 1;
        drop(lifecycle);
        Ok(WorkerLease {
            state: Arc::clone(self),
        })
    }

    /// Closes admission and waits until every previously admitted task finishes.
    pub(crate) fn stop_and_wait(&self) -> Result<(), CpuError> {
        let mut lifecycle = self.lock()?;
        lifecycle.accepting = false;
        while lifecycle.in_flight != 0 {
            lifecycle = self
                .drained
                .wait(lifecycle)
                .map_err(|_poisoned| CpuError::StateUnavailable)?;
        }
        Ok(())
    }

    /// Returns admission and load fields from one consistent lock acquisition.
    pub(crate) fn snapshot(&self) -> Result<CpuPoolSnapshot, CpuError> {
        let lifecycle = self.lock()?;
        Ok(CpuPoolSnapshot::new(
            lifecycle.accepting,
            lifecycle.in_flight,
            self.max_in_flight,
        ))
    }

    /// Acquires lifecycle state without exposing synchronization implementation.
    fn lock(&self) -> Result<MutexGuard<'_, Lifecycle>, CpuError> {
        self.lifecycle
            .lock()
            .map_err(|_poisoned| CpuError::StateUnavailable)
    }

    /// Releases one admission slot and wakes a shutdown waiter on final drain.
    fn release(&self) {
        // Task completion runs during unwinding as well. Recovering the payload
        // here is safe because lifecycle mutations contain no panic points
        // between reading and restoring the counter invariant.
        let mut lifecycle = match self.lifecycle.lock() {
            Ok(lifecycle) => lifecycle,
            Err(poisoned) => poisoned.into_inner(),
        };
        if lifecycle.in_flight == 0 {
            return;
        }

        lifecycle.in_flight -= 1;
        if lifecycle.in_flight == 0 {
            self.drained.notify_all();
        }
    }
}

/// RAII proof that one task owns an admission slot.
pub(crate) struct WorkerLease {
    state: Arc<SharedExecutorState>,
}

impl Drop for WorkerLease {
    fn drop(&mut self) {
        self.state.release();
    }
}
