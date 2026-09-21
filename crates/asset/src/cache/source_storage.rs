//! Fixed namespace controls are adopted together when the application binds its budget.

use crate::AssetError;
use solarity_cpu::{ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

// One M2 registry, one WMO authority, one BLP authority and two retirement signals.
// These controls are constructed together before the catalog can be published.
const CONTROLS: usize = 5;
struct Record {
    bytes: usize,
    memory: Option<ByteReservation>,
}
struct Binding {
    budget: CpuStorageBudget,
    _memory: ByteReservation,
}
#[derive(Default)]
pub(crate) struct SourceStorage {
    records: Mutex<[Option<Record>; CONTROLS]>,
    binding: OnceLock<Binding>,
}
impl SourceStorage {
    fn register(self: &Arc<Self>, bytes: usize) -> ControlOwner {
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|_| unreachable!("control metadata contains no domain code"));
        assert!(
            self.binding.get().is_none(),
            "namespace controls precede budget configuration"
        );
        let index = records
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| unreachable!("fixed namespace control census"));
        records[index] = Some(Record {
            bytes,
            memory: None,
        });
        ControlOwner {
            storage: Arc::clone(self),
            index,
        }
    }
    /// No ledger or root changes become visible unless the entire startup census fits.
    fn configure(&self, budget: CpuStorageBudget) -> Result<(), AssetError> {
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|_| unreachable!("control metadata contains no domain code"));
        if self.binding.get().is_some() {
            return Err(AssetError::SourceStorageConfigured);
        }
        let own_bytes = size_of::<Self>() + 2 * size_of::<usize>();
        let total = records
            .iter()
            .flatten()
            .try_fold(own_bytes, |sum, record| {
                sum.checked_add(record.bytes)
                    .ok_or(CpuError::StorageSizeOverflow)
            })?;
        let mut admission = budget.reserve_working_set(CpuStorageClass::Required, total)?;
        let memory = admission.reserve(CpuStorageKind::Metadata, own_bytes)?;
        let mut charges: [Option<ByteReservation>; CONTROLS] = std::array::from_fn(|_| None);
        for (record, charge) in records.iter().zip(&mut charges) {
            if let Some(record) = record {
                *charge = Some(admission.reserve(CpuStorageKind::Metadata, record.bytes)?);
            }
        }
        for (record, charge) in records.iter_mut().zip(charges) {
            if let Some(record) = record {
                record.memory = charge;
            }
        }
        self.binding
            .set(Binding {
                budget,
                _memory: memory,
            })
            .unwrap_or_else(|_| unreachable!("configuration holds the control lock"));
        Ok(())
    }
}

/// Every fixed allocation has one non-clonable lifetime owner; service clones share it.
pub(super) struct ControlOwner {
    storage: Arc<SourceStorage>,
    index: usize,
}
impl ControlOwner {
    pub(super) fn for_arc<T>(storage: &Arc<SourceStorage>) -> Self {
        storage.register(size_of::<T>() + 2 * size_of::<usize>())
    }
    pub(super) fn budget(&self) -> Option<&CpuStorageBudget> {
        self.storage.binding.get().map(|binding| &binding.budget)
    }
    pub(super) fn configure(&self, budget: CpuStorageBudget) -> Result<(), AssetError> {
        self.storage.configure(budget)
    }
}
impl Drop for ControlOwner {
    fn drop(&mut self) {
        let record = self
            .storage
            .records
            .lock()
            .unwrap_or_else(|_| unreachable!("control metadata contains no domain code"))
            [self.index]
            .take();
        // Return ledger ownership outside the namespace control lock.
        drop(record);
    }
}

struct Signal {
    changed: AtomicBool,
    alive: AtomicBool,
    _owner: ControlOwner,
}
/// Observers retain the small signal allocation, never a namespace/cache owner.
pub(super) struct RetirementSignal(Arc<Signal>);
impl RetirementSignal {
    pub(super) fn new(storage: &Arc<SourceStorage>) -> Self {
        Self(Arc::new(Signal {
            changed: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            _owner: ControlOwner::for_arc::<Signal>(storage),
        }))
    }
    pub(super) fn observer(&self) -> SignalObserver {
        SignalObserver(Arc::clone(&self.0))
    }
}
impl std::ops::Deref for RetirementSignal {
    type Target = AtomicBool;
    fn deref(&self) -> &AtomicBool {
        &self.0.changed
    }
}
impl Drop for RetirementSignal {
    fn drop(&mut self) {
        self.0.alive.store(false, Ordering::Release);
    }
}
pub(super) struct SignalObserver(Arc<Signal>);
impl SignalObserver {
    pub(super) fn matches(&self, signal: &RetirementSignal) -> bool {
        Arc::ptr_eq(&self.0, &signal.0)
    }
    pub(super) fn is_alive(&self) -> bool {
        self.0.alive.load(Ordering::Acquire)
    }
    pub(super) fn notify(&self) {
        self.0.changed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "../../tests/cache/source_storage.rs"]
mod tests;
