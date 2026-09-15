//! Detached CPU allocations wait for admission without blocking presentation.

use solarity_cpu::{CpuError, CpuExecutor};
use std::ops::ControlFlow;

// A service turn bounds the number of independent retirements, not their wall
// time. One large object can still contain an indivisible allocator/library free.
const RETIREMENTS_PER_STEP: usize = 16;

#[cfg(test)]
#[path = "../../tests/application/ground_detail_retirement.rs"]
mod tests;

/// Keeps detached CPU owners until bounded service admission transfers them.
pub(super) struct CpuRetirementQueue<T> {
    pending: Vec<T>,
}

impl<T: Send + 'static> CpuRetirementQueue<T> {
    pub(super) const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Adds detached owners without running their destructors during presentation.
    pub(super) fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        self.pending.extend(items);
    }

    /// Transfers the backlog once, then yields between bounded owner batches.
    /// Saturation preserves every input; pool shutdown drains all accepted steps.
    pub(super) fn service(&mut self, cpu: &CpuExecutor) -> Result<(), CpuError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let permit = match cpu.try_reserve_for(solarity_cpu::CpuService::Retirement) {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error),
        };
        // Reserve before moving ownership: saturation must never run the
        // destructor inside a rejected closure on the presentation thread.
        let mut pending = std::mem::take(&mut self.pending).into_iter();
        // The executor retains this finite task even after its result handle
        // is discarded, and shutdown waits for every admitted destructor.
        drop(permit.submit_steps(move || {
            for _ in 0..RETIREMENTS_PER_STEP {
                let Some(retired) = pending.next() else {
                    return ControlFlow::Break(());
                };
                drop(retired);
            }
            if pending.len() == 0 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }));
        Ok(())
    }
}
