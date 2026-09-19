//! Executor-owned shutdown of admitted frame producers and external dependencies.

use crate::storage::StorageVec;
use crate::{CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU64, Ordering},
};

/// Metadata-only shutdown boundary; finite running kernels retain their ownership.
pub(crate) trait EpochOwner: Send + Sync {
    fn stop(self: Arc<Self>, epoch: u64);
}
/// Weak epoch identity cannot keep a removed batch alive or stop its next binding.
struct Entry {
    owner: Weak<dyn EpochOwner>,
    // This separate allocation contains only identity metadata, never domain inputs.
    live: Weak<AtomicU64>,
    epoch: u64,
}
/// Admission bounds the registry, including open producers with no running jobs.
pub(super) struct Epochs {
    entries: Mutex<StorageVec<Entry>>,
    limit: usize,
}
impl Epochs {
    /// Reserves registry capacity once from the executor's admission bound.
    pub fn new(
        limit: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<Self, CpuError> {
        let mut entries = StorageVec::default();
        entries.reserve(budget, class, CpuStorageKind::Metadata, limit)?;
        Ok(Self {
            entries: Mutex::new(entries),
            limit,
        })
    }
    /// Pruning reads identity metadata without acquiring a batch lock or pinning
    /// an owner whose last release could destroy domain inputs under this lock.
    pub fn register(
        &self,
        owner: Weak<dyn EpochOwner>,
        live: Weak<AtomicU64>,
        epoch: u64,
    ) -> Result<(), CpuError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CpuError::StateUnavailable)?;
        entries.retain(|entry| {
            entry
                .live
                .upgrade()
                .is_some_and(|live| live.load(Ordering::Acquire) == entry.epoch)
        });
        if entries.len() == self.limit {
            return Err(CpuError::BatchCapacity);
        }
        entries.push(Entry { owner, live, epoch });
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
