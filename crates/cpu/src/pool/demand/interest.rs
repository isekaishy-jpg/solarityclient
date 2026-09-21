//! Demand transitions serialize count changes and queue publication without callbacks.

use super::super::CpuService;
use super::{CpuServiceInterest, Interest};
use std::sync::atomic::Ordering;

impl CpuServiceInterest {
    /// This consumer's current requirement, independent of other owners.
    #[must_use]
    pub fn service(&self) -> CpuService {
        CpuService::from_raw(self.0.service.load(Ordering::Acquire))
    }

    /// Changes only this logical consumer; other owners retain their own requirements.
    pub fn set_service(&self, service: CpuService) {
        let mut values = self
            .0
            .state
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("demand metadata cannot panic"));
        let previous = self.0.service.load(Ordering::Relaxed);
        if previous == service as u8 {
            return;
        }
        values.counts[previous as usize] -= 1;
        values.counts[service as usize] += 1;
        self.0.service.store(service as u8, Ordering::Relaxed);
        values.publish();
    }
}

impl Drop for Interest {
    fn drop(&mut self) {
        let mut values = self
            .state
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("demand metadata cannot panic"));
        values.counts[self.service.load(Ordering::Relaxed) as usize] -= 1;
        values.publish();
    }
}
