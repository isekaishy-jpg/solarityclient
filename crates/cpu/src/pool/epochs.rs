//! Executor-owned shutdown of admitted frame producers and external dependencies.

use crate::CpuError;
use std::sync::{Arc, Mutex, Weak};

/// Metadata-only shutdown boundary; finite running kernels retain their ownership.
pub(crate) trait EpochOwner: Send + Sync {
    fn live(&self, epoch: u64) -> bool;
    fn stop(self: Arc<Self>, epoch: u64);
}
/// Weak epoch identity cannot keep a removed batch alive or stop its next binding.
struct Entry {
    owner: Weak<dyn EpochOwner>,
    epoch: u64,
}
/// Admission bounds the registry, including open producers with no running jobs.
pub(super) struct Epochs {
    entries: Mutex<Vec<Entry>>,
    limit: usize,
}
impl Epochs {
    /// Reserves registry capacity once from the executor's admission bound.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::with_capacity(limit)),
            limit,
        }
    }
    /// Registration holds no epoch lock while consulting other admitted owners.
    pub fn register(&self, owner: Weak<dyn EpochOwner>, epoch: u64) -> Result<(), CpuError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CpuError::StateUnavailable)?;
        entries.retain(|entry| {
            entry
                .owner
                .upgrade()
                .is_some_and(|owner| owner.live(entry.epoch))
        });
        if entries.len() == self.limit {
            return Err(CpuError::BatchCapacity);
        }
        entries.push(Entry { owner, epoch });
        Ok(())
    }
    /// Releases the registry lock before callbacks can acquire epoch/readiness locks.
    pub fn stop(&mut self) -> Result<(), CpuError> {
        let entries = self
            .entries
            .get_mut()
            .map_err(|_| CpuError::StateUnavailable)?;
        for entry in entries.drain(..) {
            if let Some(owner) = entry.owner.upgrade() {
                owner.stop(entry.epoch);
            }
        }
        Ok(())
    }
}
