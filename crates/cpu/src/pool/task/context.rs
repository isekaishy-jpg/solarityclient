//! One admitted service identity and cooperative withdrawal survive every turn.

use crate::{
    ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind, JobContext,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Mandatory service metadata remains charged while either producer or consumer
/// retains it. Domain captures and result payloads need their own admission.
pub(in crate::pool) struct TaskControl {
    pub finished: AtomicBool,
    cancelled: AtomicBool,
    memory: ByteReservation,
}

impl TaskControl {
    /// Admit before caller inputs move. Shared control is required infrastructure,
    /// like service queues; changing demand does not move its accounting class.
    pub fn reserve(budget: &CpuStorageBudget) -> Result<Arc<Self>, CpuError> {
        let bytes = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        let memory = budget.reserve(CpuStorageClass::Required, CpuStorageKind::Metadata, bytes)?;
        Ok(Arc::new(Self {
            finished: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            memory,
        }))
    }

    /// Context borrows this admitted identity only for the synchronous call.
    pub fn context(&self, trace: solarity_profiling::TraceContext) -> JobContext<'_> {
        JobContext::new(self.memory.allocation_id(), 0, &self.cancelled, trace)
    }

    /// Withdrawal is monotonic; only the domain decides a safe stopping boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Dropping a consumer requests withdrawal without detaching executor ownership.
pub(in crate::pool) struct TaskInterest(pub Arc<TaskControl>);

impl Drop for TaskInterest {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
