//! One producer binds to registered demand before or after its consumers arrive.

use super::super::{CpuService, CpuServiceControl};
use super::{CpuServiceDemand, CpuServiceInterest, Interest};
use std::sync::{Arc, atomic::AtomicU8};

impl CpuServiceDemand {
    /// Strongest currently registered consumer; an empty owner has no resource demand.
    #[must_use]
    pub fn strongest(&self) -> Option<CpuService> {
        let values = self
            .0
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("demand metadata cannot panic"));
        [
            CpuService::Required,
            CpuService::Retirement,
            CpuService::Speculative,
        ]
        .into_iter()
        .find(|service| values.counts[*service as usize] != 0)
    }

    /// Registers a logical consumer; cloning its returned handle shares that registration.
    #[must_use]
    pub fn subscribe(&self, service: CpuService) -> CpuServiceInterest {
        let mut values = self
            .0
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("demand metadata cannot panic"));
        values.counts[service as usize] += 1;
        values.publish();
        CpuServiceInterest(Arc::new(Interest {
            state: Arc::clone(&self.0),
            service: AtomicU8::new(service as u8),
        }))
    }

    /// Installs the sole producer control and immediately applies existing consumer demand.
    /// Returns false if this demand owner already has a producer; existing control is preserved.
    #[must_use]
    pub fn bind(&self, control: CpuServiceControl) -> bool {
        let mut values = self
            .0
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("demand metadata cannot panic"));
        if values.control.is_some() {
            return false;
        }
        values.control = Some(control);
        values.publish();
        true
    }
}
