//! Scheduling demand is independent of consumer withdrawal and result ownership.

use crate::pool::{CpuService, dispatch::Dispatch};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicU8, Ordering},
};

/// Scheduling control can be shared without sharing consumption of the task result.
#[derive(Clone)]
pub struct CpuServiceControl {
    pub(super) dispatch: Weak<Dispatch>,
    pub(super) service: Arc<AtomicU8>,
}

impl CpuServiceControl {
    /// Creates a control for one admitted service identity, never for future epochs.
    pub(in crate::pool) fn new(dispatch: &Arc<Dispatch>, service: Arc<AtomicU8>) -> Self {
        Self {
            dispatch: Arc::downgrade(dispatch),
            service,
        }
    }

    /// Changes queued service metadata; no domain callback or worker wait is involved.
    pub fn set_service(&self, service: CpuService) {
        if self.service.load(Ordering::Acquire) != service as u8
            && let Some(dispatch) = self.dispatch.upgrade()
        {
            dispatch.reclassify(&self.service, service);
        }
    }

    /// Reports the current scheduling class of this task identity.
    #[must_use]
    pub fn service(&self) -> CpuService {
        CpuService::from_raw(self.service.load(Ordering::Acquire))
    }
}
