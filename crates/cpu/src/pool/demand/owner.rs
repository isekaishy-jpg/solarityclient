//! One producer binds to registered demand before or after its consumers arrive.

use super::super::{CpuService, CpuServiceControl};
use super::{CpuServiceDemand, CpuServiceInterest, Interest};
use std::sync::{Arc, atomic::AtomicU8};

impl CpuServiceDemand {
    /// Admits a shared demand owner before allocating its control metadata.
    /// # Errors
    /// Returns byte pressure without creating or binding demand.
    pub fn admitted(budget: &crate::CpuStorageBudget) -> Result<Self, crate::CpuError> {
        let memory = budget.reserve(
            crate::CpuStorageClass::Required,
            crate::CpuStorageKind::Metadata,
            size_of::<super::State>() + 2 * size_of::<usize>(),
        )?;
        Ok(Self(Arc::new(super::State {
            values: Default::default(),
            _memory: Some(memory),
        })))
    }

    /// Admits one logical consumer; cloned handles share the same allocation charge.
    /// # Errors
    /// Returns byte pressure before changing live producer demand.
    pub fn subscribe_admitted(
        &self,
        service: CpuService,
        budget: &crate::CpuStorageBudget,
    ) -> Result<CpuServiceInterest, crate::CpuError> {
        let memory = budget.reserve(
            crate::CpuStorageClass::Required,
            crate::CpuStorageKind::Metadata,
            size_of::<Interest>() + 2 * size_of::<usize>(),
        )?;
        Ok(self.subscribe_inner(service, Some(memory)))
    }
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
        self.subscribe_inner(service, None)
    }

    fn subscribe_inner(
        &self,
        service: CpuService,
        memory: Option<crate::ByteReservation>,
    ) -> CpuServiceInterest {
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
            _memory: memory,
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
