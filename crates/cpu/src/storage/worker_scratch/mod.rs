//! Versioned, admitted worker lanes share temporary storage across compatible jobs.

mod scope;
mod slots;

use super::{ByteReservation, CpuStorageClass, CpuStorageKind, StorageVec};
use crate::{CpuError, CpuExecutor};
use slots::Lane;
use std::sync::{Arc, Weak};

/// One typed operation's reusable scratch lanes. Clones pin the admitted version,
/// not a fresh allocation. Growth and explicit trimming publish a replacement;
/// already dispatched work keeps its old version until its final owner returns.
/// Nested allocations in temporary values remain the domain's responsibility.
pub struct CpuWorkerScratch<T> {
    core: Arc<Core<T>>,
}

/// The weak executor identity prevents address reuse without retaining its threads.
struct Core<T> {
    owner: Weak<crate::pool::Dispatch>,
    budget: super::CpuStorageBudget,
    class: CpuStorageClass,
    capacity: usize,
    lanes: StorageVec<Lane<T>>,
    _memory: ByteReservation,
}

impl<T> Clone for CpuWorkerScratch<T> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

impl<T> CpuWorkerScratch<T> {
    /// Registers one empty lane per physical worker, charging all control metadata.
    /// `reserve` admits a complete version before input or simulation transfer.
    ///
    /// # Errors
    /// Reports byte, arithmetic or allocator refusal without creating a binding.
    pub fn new(cpu: &CpuExecutor, class: CpuStorageClass) -> Result<Self, CpuError> {
        Self::build(
            cpu.worker_owner(),
            cpu.storage().clone(),
            class,
            cpu.worker_count(),
            0,
        )
    }

    /// Allocates a whole replacement while the old version remains charged.
    fn build(
        owner: Weak<crate::pool::Dispatch>,
        budget: super::CpuStorageBudget,
        class: CpuStorageClass,
        workers: usize,
        capacity: usize,
    ) -> Result<Self, CpuError> {
        let memory = budget.reserve(
            class,
            CpuStorageKind::Metadata,
            std::mem::size_of::<Core<T>>() + 2 * std::mem::size_of::<usize>(),
        )?;
        let mut lanes = StorageVec::new();
        lanes.reserve(&budget, class, CpuStorageKind::Metadata, workers)?;
        for _ in 0..workers {
            lanes.push(Lane::new(&budget, class, capacity)?);
        }
        Ok(Self {
            core: Arc::new(Core {
                owner,
                budget,
                class,
                capacity,
                lanes,
                _memory: memory,
            }),
        })
    }

    /// Checks identity only; a worker index from another executor is never enough.
    #[must_use]
    pub fn belongs_to(&self, cpu: &CpuExecutor) -> bool {
        Weak::ptr_eq(&self.core.owner, &cpu.worker_owner())
    }

    /// Guaranteed element capacity of every lane in this admitted version.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.core.capacity
    }

    /// Admits every worker's complete bound before replacing this version.
    /// Existing clones retain their original allocation and bound on success.
    ///
    /// # Errors
    /// Failure preserves this version, its charge and every in-flight clone.
    pub fn reserve(&mut self, capacity: usize) -> Result<(), CpuError> {
        if capacity > self.capacity() {
            self.replace(capacity)?;
        }
        Ok(())
    }

    /// Explicit maintenance reduces warm capacity. Old in-flight versions remain
    /// charged until released; trimming cannot revoke a running operation's loan.
    ///
    /// # Errors
    /// Reports inability to admit the replacement while old versions are pinned.
    pub fn trim(&mut self, capacity: usize) -> Result<(), CpuError> {
        if capacity < self.capacity() {
            self.replace(capacity)?;
        }
        Ok(())
    }

    /// Replacement is transactional even when a later lane's allocation fails.
    fn replace(&mut self, capacity: usize) -> Result<(), CpuError> {
        *self = Self::build(
            self.core.owner.clone(),
            self.core.budget.clone(),
            self.core.class,
            self.core.lanes.len(),
            capacity,
        )?;
        Ok(())
    }

    /// Largest requested loan per worker in this allocation generation. This is
    /// declared operation demand, not process RSS or a scan of temporary values.
    pub fn worker_peaks(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.core.lanes.iter().map(|lane| lane.peak())
    }
}
