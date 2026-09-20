//! Fixed cost buckets for admitted runner records; no domain state is inspected.

use super::{QueuedWork as Work, ReadyWork};
use crate::storage::StorageDeque;
use crate::{CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::Arc;

/// Each bucket can receive the entire admitted runner bound after reclassification.
/// The total live record count is still bounded by the existing frame admissions.
#[derive(Default)]
pub(super) struct CostQueue {
    bins: [StorageDeque<Work>; 3],
}

impl CostQueue {
    /// Reserves before workers start; classification never allocates during a frame.
    pub fn reserve(&mut self, budget: &CpuStorageBudget, capacity: usize) -> Result<(), CpuError> {
        for bin in &mut self.bins {
            bin.reserve(
                budget,
                CpuStorageClass::Frame,
                CpuStorageKind::Metadata,
                capacity,
            )?;
        }
        Ok(())
    }

    /// Reads only an atomic class, under the same queue lock as cost promotion.
    pub fn push(&mut self, work: Work) {
        self.bins[usize::from(work.cost())].push_back(work);
    }

    /// Repairs a decreasing hint before choosing another ready runner. Increases
    /// also notify/reclassify under this queue lock, so they cannot remain buried.
    pub fn pop(&mut self) -> Option<Work> {
        loop {
            let bin = self.bins.iter().rposition(|bin| !bin.is_empty())?;
            let work = self.bins[bin]
                .pop_front()
                .unwrap_or_else(|| unreachable!("selected cost bin is nonempty"));
            if usize::from(work.cost()) == bin {
                return Some(work);
            }
            self.push(work);
        }
    }

    /// Zero is empty; 1..=3 encode the highest queued class for boundary yielding.
    pub fn level(&self) -> u8 {
        self.bins
            .iter()
            .rposition(|bin| !bin.is_empty())
            .map_or(0, |bin| bin as u8 + 1)
    }

    pub fn is_empty(&self) -> bool {
        self.level() == 0
    }

    /// Moves only this phase's queued records, preserving FIFO ties in each bin.
    /// The scan is over bounded runner reservations, never the scene's job list.
    pub fn promote(&mut self, owner: &Arc<dyn ReadyWork>, destination: &mut Self) {
        for bin in &mut self.bins {
            for _ in 0..bin.len() {
                let work = bin
                    .pop_front()
                    .unwrap_or_else(|| unreachable!("promotion retains its scan bound"));
                if work.belongs_to(owner) {
                    destination.push(work);
                } else {
                    bin.push_back(work);
                }
            }
        }
    }

    /// An increase may happen after all runners were queued. Reclassify those
    /// existing reservations without duplicating them or changing their urgency.
    pub fn reclassify(&mut self, owner: &Arc<dyn ReadyWork>) {
        for bin in 0..self.bins.len() {
            for _ in 0..self.bins[bin].len() {
                let work = self.bins[bin]
                    .pop_front()
                    .unwrap_or_else(|| unreachable!("cost scan retains its bound"));
                if work.belongs_to(owner) {
                    self.push(work);
                } else {
                    self.bins[bin].push_back(work);
                }
            }
        }
    }
}
