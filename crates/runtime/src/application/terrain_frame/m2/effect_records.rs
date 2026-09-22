//! Fixed placement-local records retain their capacity charge during worker transfer.

use solarity_cpu::{
    ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use std::ops::{Deref, DerefMut};

/// Builders supply authored records once. Frame access cannot grow their array;
/// nested simulation pools retain their own independent capacity charges.
pub(super) struct EffectRecords<T> {
    values: Vec<T>,
    memory: Option<ByteReservation>,
}

impl<T> Default for EffectRecords<T> {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl<T> From<Vec<T>> for EffectRecords<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            values,
            memory: None,
        }
    }
}

impl<T> EffectRecords<T> {
    fn bytes(&self) -> Result<usize, CpuError> {
        self.values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
    }

    /// Count actual retained capacity before either records or input copies transfer.
    pub(super) fn include_storage(
        &self,
        budget: &CpuStorageBudget,
        working_set: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        let bytes = match &self.memory {
            Some(memory) => memory.admission_bytes(budget, Class::Frame),
            None => self.bytes()?,
        };
        working_set.include(bytes, 0)
    }

    /// Adoption moves no records and retains identity when changing executors.
    pub(super) fn reserve_reserved(
        &mut self,
        reservation: &mut CpuStorageReservation,
    ) -> Result<(), CpuError> {
        if let Some(memory) = &mut self.memory {
            memory.transfer_reserved(reservation, Kind::Scratch)
        } else {
            let bytes = self.bytes()?;
            if bytes != 0 {
                self.memory = Some(reservation.reserve(Kind::Scratch, bytes)?);
            }
            Ok(())
        }
    }
}

impl<T> Deref for EffectRecords<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}
impl<T> DerefMut for EffectRecords<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}
impl<'a, T> IntoIterator for &'a EffectRecords<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut EffectRecords<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solarity_cpu::CpuStoragePlan;

    #[test]
    fn capacity_charge_follows_records_through_transfer_and_refusal() -> Result<(), CpuError> {
        let mut values = Vec::with_capacity(17);
        values.extend([3_u64, 5, 8]);
        let bytes = values.capacity() * size_of::<u64>();
        let address = values.as_ptr();
        let mut placement = EffectRecords::from(values);
        let refused = CpuStorageBudget::new(CpuStoragePlan::new(bytes - 1, 0, 0));
        let mut plan = CpuStorageWorkingSet::default();
        placement.include_storage(&refused, &mut plan)?;
        assert_eq!(plan.bytes(), bytes);
        assert!(
            refused
                .reserve_working_set(Class::Frame, plan.bytes())
                .is_err()
        );
        assert!(placement.memory.is_none());
        assert_eq!(placement.as_ptr(), address);
        assert_eq!(&*placement, &[3, 5, 8]);

        let budget = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
        placement.reserve_reserved(&mut budget.reserve_working_set(Class::Frame, bytes)?)?;
        let identity = placement
            .memory
            .as_ref()
            .ok_or(CpuError::StorageAllocation)?
            .allocation_id();
        let mut worker = EffectRecords::default();
        for _ in 0..100 {
            std::mem::swap(&mut placement, &mut worker);
            let mut plan = CpuStorageWorkingSet::default();
            worker.include_storage(&budget, &mut plan)?;
            assert_eq!(plan.bytes(), 0);
            worker.reserve_reserved(&mut budget.reserve_working_set(Class::Frame, 0)?)?;
            assert_eq!(worker.as_ptr(), address);
            assert_eq!(
                worker
                    .memory
                    .as_ref()
                    .ok_or(CpuError::StorageAllocation)?
                    .allocation_id(),
                identity
            );
            std::mem::swap(&mut placement, &mut worker);
        }
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        assert!(
            placement
                .reserve_reserved(&mut refused.reserve_working_set(Class::Frame, bytes - 1)?)
                .is_err()
        );
        assert_eq!(budget.snapshot().used(Class::Frame), bytes);
        assert_eq!(refused.snapshot().used(Class::Frame), 0);
        assert_eq!(
            placement
                .memory
                .as_ref()
                .ok_or(CpuError::StorageAllocation)?
                .allocation_id(),
            identity
        );

        let destination = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
        placement.reserve_reserved(&mut destination.reserve_working_set(Class::Frame, bytes)?)?;
        assert_eq!(budget.snapshot().used(Class::Frame), 0);
        assert_eq!(destination.snapshot().used(Class::Frame), bytes);
        assert_eq!(
            placement
                .memory
                .as_ref()
                .ok_or(CpuError::StorageAllocation)?
                .allocation_id(),
            identity
        );
        assert_eq!(placement.as_ptr(), address);
        drop(placement);
        assert_eq!(destination.snapshot().used(Class::Frame), 0);
        Ok(())
    }
}
