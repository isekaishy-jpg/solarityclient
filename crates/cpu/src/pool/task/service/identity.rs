//! An admitted service retains one queue identity across all demand owners.

use crate::pool::{CpuService, dispatch::Dispatch};
use crate::{ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::{Arc, Mutex, Weak, atomic::AtomicU8};

/// Queue readers only inspect the atomic class; domain transitions own the counts.
pub(crate) struct ServiceIdentity {
    pub(in crate::pool) effective: AtomicU8,
    dispatch: Weak<Dispatch>,
    values: Mutex<Values>,
    pub(super) budget: CpuStorageBudget,
    _memory: ByteReservation,
}

/// The task's own requirement survives changes and withdrawal of nested sources.
struct Values {
    owner: CpuService,
    scopes: [usize; 3],
}

impl Values {
    /// Enum order matches scheduling precedence: required, retirement, speculative.
    fn effective(&self) -> CpuService {
        let mut selected = self.owner as usize;
        for (class, count) in self.scopes.iter().enumerate() {
            if *count != 0 {
                selected = selected.min(class);
            }
        }
        CpuService::from_raw(selected as u8)
    }
}

impl ServiceIdentity {
    /// Created once per admitted task or loading epoch, never per service turn.
    pub(in crate::pool) fn reserve(
        dispatch: &Arc<Dispatch>,
        service: CpuService,
        budget: &CpuStorageBudget,
    ) -> Result<Arc<Self>, CpuError> {
        let bytes = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        let memory = budget.reserve(CpuStorageClass::Required, CpuStorageKind::Metadata, bytes)?;
        Ok(Arc::new(Self {
            effective: AtomicU8::new(service as u8),
            dispatch: Arc::downgrade(dispatch),
            values: Mutex::new(Values {
                owner: service,
                scopes: [0; 3],
            }),
            budget: budget.clone(),
            _memory: memory,
        }))
    }

    /// Queue propagation occurs without holding identity metadata: a suspended
    /// task may promote a prerequisite, which can share its producer's identity.
    pub(super) fn publish(self: &Arc<Self>) {
        let Some(dispatch) = self.dispatch.upgrade() else {
            return;
        };
        loop {
            let selected = self
                .values
                .lock()
                .unwrap_or_else(|_| unreachable!("service metadata cannot panic"))
                .effective();
            dispatch.reclassify(self, selected);
            // A concurrent update can overtake queue publication. Reconcile to
            // current requirements before returning instead of publishing stale demand.
            if self
                .values
                .lock()
                .unwrap_or_else(|_| unreachable!("service metadata cannot panic"))
                .effective()
                == selected
            {
                return;
            }
        }
    }

    /// Owner transitions never reset scoped producer demand.
    pub(super) fn set_service(self: &Arc<Self>, service: CpuService) {
        self.values
            .lock()
            .unwrap_or_else(|_| unreachable!("service metadata cannot panic"))
            .owner = service;
        self.publish();
    }

    /// Contribution state is locked before these counts. Dispatch never takes either lock.
    pub(super) fn change_scope(&self, previous: Option<CpuService>, next: Option<CpuService>) {
        let mut values = self
            .values
            .lock()
            .unwrap_or_else(|_| unreachable!("service metadata cannot panic"));
        if let Some(previous) = previous {
            values.scopes[previous as usize] -= 1;
        }
        if let Some(next) = next {
            values.scopes[next as usize] += 1;
        }
    }
}
