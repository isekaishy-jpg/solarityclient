//! RAII byte reservations with stable allocation identities and atomic transfers.

use super::{CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot};
use crate::CpuError;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

/// Process-wide opaque identity; it carries no global cache or domain ownership.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
thread_local! { static LEDGER_LOCKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

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
        #[cfg(test)]
        LEDGER_LOCKS.with(|locks| locks.set(locks.get() + 1));
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
        if bytes != 0 {
            self.lock().add(class, kind, bytes)?;
        }
        Ok(ByteReservation {
            budget: self.clone(),
            class,
            kind,
            bytes,
            id,
        })
    }
    /// Protects a connected working set before any of its individual allocations grow.
    /// Unassigned capacity is scratch; each allocation takes its final purpose when funded.
    /// # Errors
    /// An infeasible total leaves all existing allocations and charges unchanged.
    pub fn reserve_working_set(
        &self,
        class: CpuStorageClass,
        bytes: usize,
    ) -> Result<CpuStorageReservation, CpuError> {
        if bytes != 0 {
            self.lock().add(class, CpuStorageKind::Scratch, bytes)?;
        }
        Ok(CpuStorageReservation {
            // A funding scope is not a backing allocation. Only its children issue
            // allocation identities; warmed empty scopes need no global transaction.
            memory: ByteReservation {
                budget: self.clone(),
                class,
                kind: CpuStorageKind::Scratch,
                bytes,
                id: 0,
            },
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

    /// Additional destination headroom needed to adopt this existing allocation.
    #[must_use]
    pub fn admission_bytes(&self, destination: &CpuStorageBudget, class: CpuStorageClass) -> usize {
        if self.class == class && Arc::ptr_eq(&self.budget.ledger, &destination.ledger) {
            0
        } else {
            self.bytes
        }
    }

    /// Moves the same allocation identity using already protected destination capacity.
    /// # Errors
    /// Insufficient reserved capacity preserves the original allocation charge.
    pub fn transfer_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
        kind: CpuStorageKind,
    ) -> Result<(), CpuError> {
        if self.admission_bytes(&reservation.memory.budget, reservation.memory.class) == 0 {
            return self.transfer(&reservation.memory.budget, reservation.memory.class, kind);
        }
        let mut replacement = reservation.reserve(kind, self.bytes)?;
        replacement.id = self.id;
        *self = replacement;
        Ok(())
    }

    /// Reconciles capacity using protected headroom, without a second class admission.
    /// # Errors
    /// Refuses growth beyond the reserved working set without changing its byte count.
    pub fn resize_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
        bytes: usize,
    ) -> Result<(), CpuError> {
        self.transfer_reserved(reservation, self.kind)?;
        if bytes <= self.bytes {
            return self.resize(bytes);
        }
        reservation.consume(self.kind, bytes - self.bytes)?;
        self.bytes = bytes;
        Ok(())
    }

    /// Reconciles actual capacity. Callers free storage before shrinking the charge.
    /// # Errors
    /// Growth saturation leaves the previous reservation unchanged.
    pub fn resize(&mut self, bytes: usize) -> Result<(), CpuError> {
        if bytes == self.bytes {
            return Ok(());
        }
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
        if self.bytes == 0 {
            if !Arc::ptr_eq(&self.budget.ledger, &destination.ledger) {
                self.budget = destination.clone();
            }
            self.class = class;
            self.kind = kind;
            return Ok(());
        }
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
        if self.bytes != 0 {
            self.budget.lock().remove(self.class, self.kind, self.bytes);
        }
    }
}

/// Exclusive, preadmitted capacity for a connected transaction's allocations.
/// Splitting does not double-charge or expose protected headroom to competing work.
/// Unused capacity returns on drop; funded allocations retain independent ownership.
pub struct CpuStorageReservation {
    memory: ByteReservation,
}
impl CpuStorageReservation {
    /// Capacity still available to this transaction, already held against its class limit.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.memory.bytes
    }

    /// Funds one allocation while preserving the complete transaction's admission.
    /// # Errors
    /// Refuses an underestimated working set or exhausted allocation identities.
    pub fn reserve(
        &mut self,
        kind: CpuStorageKind,
        bytes: usize,
    ) -> Result<ByteReservation, CpuError> {
        self.require(bytes)?;
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| CpuError::EpochExhausted)?;
        self.consume(kind, bytes)?;
        Ok(ByteReservation {
            budget: self.memory.budget.clone(),
            class: self.memory.class,
            kind,
            bytes,
            id,
        })
    }
    /// Reuses a retired allocation's charge as protected capacity for later replacements.
    /// The caller must free its backing allocation before returning the charge.
    /// # Errors
    /// Foreign adoption can fail; normal same-transaction retirement needs no headroom.
    pub fn recycle(&mut self, mut memory: ByteReservation) -> Result<(), CpuError> {
        let kind = self.memory.kind;
        memory.transfer_reserved(self, kind)?;
        self.memory.bytes += memory.bytes;
        memory.bytes = 0;
        Ok(())
    }

    fn require(&self, bytes: usize) -> Result<(), CpuError> {
        if bytes > self.memory.bytes {
            return Err(CpuError::StorageAtCapacity {
                class: self.memory.class,
                requested: bytes,
                available: self.memory.bytes,
            });
        }
        Ok(())
    }
    fn consume(&mut self, kind: CpuStorageKind, bytes: usize) -> Result<(), CpuError> {
        self.require(bytes)?;
        if bytes == 0 {
            return Ok(());
        }
        if kind != self.memory.kind {
            let mut ledger = self.memory.budget.lock();
            ledger.remove(self.memory.class, self.memory.kind, bytes);
            ledger.used[self.memory.class as usize][kind as usize] += bytes;
        }
        self.memory.bytes -= bytes;
        Ok(())
    }
}

/// Computes peak additional headroom in the exact allocation/retirement order.
/// Retired old buffers fund later replacements without exposing their space to competitors.
#[derive(Default)]
pub struct CpuStorageWorkingSet {
    live: usize,
    peak: usize,
}
impl CpuStorageWorkingSet {
    /// Includes one stage's admission followed by retirement of its old allocation.
    /// # Errors
    /// Reports overflow or an impossible retirement before issuing any reservation.
    pub fn include(
        &mut self,
        admission_bytes: usize,
        retired_bytes: usize,
    ) -> Result<(), CpuError> {
        let peak = self
            .live
            .checked_add(admission_bytes)
            .ok_or(CpuError::StorageSizeOverflow)?;
        let live = peak
            .checked_sub(retired_bytes)
            .ok_or(CpuError::StorageSizeOverflow)?;
        self.peak = self.peak.max(peak);
        self.live = live;
        Ok(())
    }
    /// Additional bytes which must be protected before the first included stage starts.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.peak
    }
}

#[cfg(test)]
#[path = "../../tests/internal/storage_reuse.rs"]
mod tests;
