//! RAII byte reservations with stable allocation identities and atomic transfers.

use super::{CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot};
use crate::CpuError;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

/// Process-wide opaque identity; it carries no global cache or domain ownership.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Short accounting transactions never execute allocation or domain destructors.
struct Ledger {
    limits: [usize; 3],
    used: [[usize; 3]; 3],
    peaks: [usize; 3],
}
impl Ledger {
    /// Tests admission before changing either category, including checked sums.
    fn add(
        &mut self,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        bytes: usize,
    ) -> Result<(), CpuError> {
        let index = class as usize;
        let used: usize = self.used[index].iter().sum();
        if bytes > self.limits[index].saturating_sub(used) {
            return Err(CpuError::StorageAtCapacity {
                class,
                requested: bytes,
                available: self.limits[index].saturating_sub(used),
            });
        }
        self.used[index][kind as usize] += bytes;
        self.peaks[index] = self.peaks[index].max(used + bytes);
        Ok(())
    }
    /// Every release has a unique owning reservation; underflow is an internal defect.
    fn remove(&mut self, class: CpuStorageClass, kind: CpuStorageKind, bytes: usize) {
        self.used[class as usize][kind as usize] -= bytes;
    }
}

/// Shared accounting survives executor teardown while retained pages remain pinned.
#[derive(Clone)]
pub struct CpuStorageBudget {
    ledger: Arc<Mutex<Ledger>>,
}
impl CpuStorageBudget {
    /// Creates empty accounting from explicit application policy.
    #[must_use]
    pub fn new(plan: CpuStoragePlan) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(Ledger {
                limits: plan.limits,
                used: [[0; 3]; 3],
                peaks: [0; 3],
            })),
        }
    }
    /// Accounting never calls user code, so poison indicates an internal defect.
    fn lock(&self) -> MutexGuard<'_, Ledger> {
        self.ledger
            .lock()
            .unwrap_or_else(|_| unreachable!("storage accounting contains no user operation"))
    }
    /// Reserves before transferring inputs or attempting allocation.
    /// # Errors
    /// Reports class saturation or exhausted allocation identities without charging bytes.
    pub fn reserve(
        &self,
        class: CpuStorageClass,
        kind: CpuStorageKind,
        bytes: usize,
    ) -> Result<ByteReservation, CpuError> {
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| CpuError::EpochExhausted)?;
        self.lock().add(class, kind, bytes)?;
        Ok(ByteReservation {
            budget: self.clone(),
            class,
            kind,
            bytes,
            id,
        })
    }
    /// Samples all categories together under the accounting lock.
    #[must_use]
    pub fn snapshot(&self) -> CpuStorageSnapshot {
        let ledger = self.lock();
        CpuStorageSnapshot {
            limits: ledger.limits,
            used: ledger.used,
            peaks: ledger.peaks,
        }
    }
}

/// Unique charge for one allocation. Moving/sharing a payload never clones this charge.
pub struct ByteReservation {
    budget: CpuStorageBudget,
    class: CpuStorageClass,
    kind: CpuStorageKind,
    bytes: usize,
    id: u64,
}
impl ByteReservation {
    /// Opaque identity retained when an allocation moves between budgets.
    #[must_use]
    pub const fn allocation_id(&self) -> u64 {
        self.id
    }
    /// Capacity currently charged by this unique reservation.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Reconciles actual capacity. Callers free storage before shrinking the charge.
    /// # Errors
    /// Growth saturation leaves the previous reservation unchanged.
    pub fn resize(&mut self, bytes: usize) -> Result<(), CpuError> {
        let mut ledger = self.budget.lock();
        if bytes >= self.bytes {
            ledger.add(self.class, self.kind, bytes - self.bytes)?;
        } else {
            ledger.remove(self.class, self.kind, self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    /// Transfers the same allocation identity without a transient double charge.
    /// Cross-ledger locks are acquired by stable Arc allocation address; neither
    /// ledger invokes external code while both are held.
    /// # Errors
    /// Destination saturation preserves the original class, ledger and identity.
    pub fn transfer(
        &mut self,
        destination: &CpuStorageBudget,
        class: CpuStorageClass,
        kind: CpuStorageKind,
    ) -> Result<(), CpuError> {
        if Arc::ptr_eq(&self.budget.ledger, &destination.ledger) {
            if class == self.class && kind == self.kind {
                return Ok(());
            }
            let mut ledger = destination.lock();
            if class == self.class {
                ledger.remove(self.class, self.kind, self.bytes);
                ledger.used[class as usize][kind as usize] += self.bytes;
            } else {
                ledger.add(class, kind, self.bytes)?;
                ledger.remove(self.class, self.kind, self.bytes);
            }
        } else {
            if Arc::as_ptr(&self.budget.ledger) < Arc::as_ptr(&destination.ledger) {
                let mut source = self.budget.lock();
                let mut target = destination.lock();
                target.add(class, kind, self.bytes)?;
                source.remove(self.class, self.kind, self.bytes);
            } else {
                let mut target = destination.lock();
                let mut source = self.budget.lock();
                target.add(class, kind, self.bytes)?;
                source.remove(self.class, self.kind, self.bytes);
            }
            self.budget = destination.clone();
        }
        self.class = class;
        self.kind = kind;
        Ok(())
    }
}
impl Drop for ByteReservation {
    fn drop(&mut self) {
        self.budget.lock().remove(self.class, self.kind, self.bytes);
    }
}
