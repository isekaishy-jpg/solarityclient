//! Task demand combines its owner with scoped requirements of nested producers.

mod identity;
mod scope;

pub(in crate::pool) use identity::ServiceIdentity;
pub use scope::CpuServiceScope;

use crate::pool::CpuService;
use scope::Contribution;
use std::sync::{Arc, atomic::Ordering};

/// Scheduling metadata can be shared independently of task result ownership.
#[derive(Clone)]
pub struct CpuServiceControl {
    identity: Arc<ServiceIdentity>,
    contribution: Option<Arc<Contribution>>,
}

impl CpuServiceControl {
    /// Every handle for one admitted identity shares the same owner and scopes.
    pub(in crate::pool) fn new(identity: Arc<ServiceIdentity>) -> Self {
        Self {
            identity,
            contribution: None,
        }
    }

    /// Changes this owner's demand without overriding another live requirement.
    /// A control from an expired scope has no effect on the containing task.
    pub fn set_service(&self, service: CpuService) {
        if let Some(contribution) = &self.contribution {
            contribution.set_service(service);
        } else {
            self.identity.set_service(service);
        }
    }

    /// Adds a nested producer requirement until the returned scope is dropped.
    /// Bind the scope's control to shared source demand; keep the scope only
    /// through source publication. Its consumers cannot demote the parent owner.
    /// # Errors
    /// Returns memory admission failure before registering any demand.
    pub fn scoped_demand(&self, service: CpuService) -> Result<CpuServiceScope, crate::CpuError> {
        CpuServiceScope::new(Arc::clone(&self.identity), service)
    }

    /// Reports the effective class after combining the owner and all live scopes.
    #[must_use]
    pub fn service(&self) -> CpuService {
        CpuService::from_raw(self.identity.effective.load(Ordering::Acquire))
    }
}
