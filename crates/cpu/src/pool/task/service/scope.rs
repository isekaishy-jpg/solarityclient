//! A nested source's scheduling influence ends at its producer boundary.

use super::{CpuServiceControl, ServiceIdentity};
use crate::pool::CpuService;
use crate::{ByteReservation, CpuError, CpuStorageClass, CpuStorageKind};
use std::sync::{Arc, Mutex};

/// Cloned controls share one contribution; only the scope owns its lifetime.
pub(super) struct Contribution {
    identity: Arc<ServiceIdentity>,
    service: Mutex<Option<CpuService>>,
    _memory: ByteReservation,
}

impl Contribution {
    /// Expired source controls cannot reclassify a later phase of the containing task.
    pub(super) fn set_service(&self, service: CpuService) {
        let mut current = self
            .service
            .lock()
            .unwrap_or_else(|_| unreachable!("scoped service metadata cannot panic"));
        let Some(previous) = *current else {
            return;
        };
        if previous == service {
            return;
        }
        self.identity.change_scope(Some(previous), Some(service));
        *current = Some(service);
        drop(current);
        self.identity.publish();
    }
}

/// One producer's priority contribution, independent of the containing job's owner.
/// Dropping this scope withdraws its contribution even if consumers retain controls.
pub struct CpuServiceScope(Arc<Contribution>);

impl CpuServiceScope {
    /// Register once, with no queue-node allocation or additional admitted task.
    pub(super) fn new(
        identity: Arc<ServiceIdentity>,
        service: CpuService,
    ) -> Result<Self, CpuError> {
        let bytes = std::mem::size_of::<Contribution>() + 2 * std::mem::size_of::<usize>();
        let memory =
            identity
                .budget
                .reserve(CpuStorageClass::Required, CpuStorageKind::Metadata, bytes)?;
        identity.change_scope(None, Some(service));
        identity.publish();
        Ok(Self(Arc::new(Contribution {
            identity,
            service: Mutex::new(Some(service)),
            _memory: memory,
        })))
    }

    /// Supplies a control suitable for binding to a nested source's aggregate demand.
    #[must_use]
    pub fn control(&self) -> CpuServiceControl {
        CpuServiceControl {
            identity: Arc::clone(&self.0.identity),
            contribution: Some(Arc::clone(&self.0)),
        }
    }
}

impl Drop for CpuServiceScope {
    fn drop(&mut self) {
        let mut current = self
            .0
            .service
            .lock()
            .unwrap_or_else(|_| unreachable!("scoped service metadata cannot panic"));
        self.0.identity.change_scope(current.take(), None);
        drop(current);
        self.0.identity.publish();
    }
}
