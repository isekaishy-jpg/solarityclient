//! Detached CPU allocations wait for admission without blocking presentation.

use solarity_cpu::{CpuError, CpuExecutor};

#[cfg(test)]
#[path = "../../tests/application/ground_detail_retirement.rs"]
mod tests;

pub(super) struct CpuRetirementQueue<T> {
    pending: Vec<T>,
}

impl<T: Send + 'static> CpuRetirementQueue<T> {
    pub(super) const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub(super) fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        self.pending.extend(items);
    }

    pub(super) fn service(&mut self, cpu: &CpuExecutor) -> Result<(), CpuError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error),
        };
        // Reserve before moving ownership: saturation must never run the
        // destructor inside a rejected closure on the presentation thread.
        let pending = std::mem::take(&mut self.pending);
        // The executor retains this finite task even after its result handle
        // is discarded, and shutdown waits for every admitted destructor.
        drop(permit.submit(move || drop(pending)));
        Ok(())
    }
}
