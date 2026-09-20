//! Skeletal working storage and its charge move together through owned CPU jobs.

use super::{M2AnimationClock, M2BonePose};
use glam::Mat4;
use solarity_cpu::{ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};

/// One charge covers the three uniquely owned palette allocations, not model assets.
pub(super) struct PoseMemory(ByteReservation);

impl std::fmt::Debug for PoseMemory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("PoseMemory")
            .field(&self.0.bytes())
            .finish()
    }
}

/// Replaces storage only after its complete capacity was admitted.
fn copy_capacity<T: Copy>(values: &[T], capacity: usize) -> Result<Vec<T>, CpuError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| CpuError::StorageAllocation)?;
    result.extend_from_slice(values);
    Ok(result)
}

/// Includes local scratch and per-bone sequence selection, not just GPU matrices.
fn bytes(transforms: usize, local: usize, sequences: usize) -> Result<usize, CpuError> {
    transforms
        .checked_add(local)
        .and_then(|count| count.checked_mul(size_of::<Mat4>()))
        .and_then(|matrices| {
            sequences
                .checked_mul(size_of::<Option<M2AnimationClock>>())
                .and_then(|clocks| matrices.checked_add(clocks))
        })
        .ok_or(CpuError::StorageSizeOverflow)
}

impl M2BonePose {
    /// Admits the complete skeletal working set before a pose enters a worker.
    /// Existing values and their charge survive refusal. Transfers and swaps of
    /// the pose also transfer its reservation; consuming matrices does not release it.
    ///
    /// # Errors
    /// Reports byte/capacity overflow, budget refusal or allocation failure.
    pub fn reserve_cpu_storage(
        &mut self,
        budget: &CpuStorageBudget,
        bones: usize,
    ) -> Result<(), CpuError> {
        let class = CpuStorageClass::Frame;
        let kind = CpuStorageKind::Result;
        if let Some(memory) = &mut self.memory {
            memory.0.transfer(budget, class, kind)?;
        } else {
            self.memory = Some(PoseMemory(budget.reserve(
                class,
                kind,
                bytes(
                    self.transforms.capacity(),
                    self.local.capacity(),
                    self.sequence_clocks.capacity(),
                )?,
            )?));
        }
        if bones <= self.transforms.capacity()
            && bones <= self.local.capacity()
            && bones <= self.sequence_clocks.capacity()
        {
            return Ok(());
        }
        let capacities = [
            bones.max(self.transforms.capacity()),
            bones.max(self.local.capacity()),
            bones.max(self.sequence_clocks.capacity()),
        ];
        let mut memory = budget.reserve(
            class,
            kind,
            bytes(capacities[0], capacities[1], capacities[2])?,
        )?;
        let transforms = copy_capacity(&self.transforms, capacities[0])?;
        let local = copy_capacity(&self.local, capacities[1])?;
        let sequences = copy_capacity(&self.sequence_clocks, capacities[2])?;
        memory.resize(bytes(
            transforms.capacity(),
            local.capacity(),
            sequences.capacity(),
        )?)?;
        self.transforms = transforms;
        self.local = local;
        self.sequence_clocks = sequences;
        self.memory = Some(PoseMemory(memory));
        Ok(())
    }

    /// Byte census follows actual retained capacity, including a returned worker pose.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.transforms.capacity() * size_of::<Mat4>()
            + self.local.capacity() * size_of::<Mat4>()
            + self.sequence_clocks.capacity() * size_of::<Option<M2AnimationClock>>()
    }

    /// Unbound offline builders retain their existing allocation behavior. A bound
    /// frame pose must stay inside its preadmitted complete model capacity.
    pub(super) fn check_cpu_storage(&self, bones: usize) -> Result<(), super::M2BonePoseError> {
        if self.memory.is_some()
            && (bones > self.transforms.capacity()
                || bones > self.local.capacity()
                || bones > self.sequence_clocks.capacity())
        {
            return Err(super::M2BonePoseError::StorageCapacity {
                requested: bones,
                available: self
                    .transforms
                    .capacity()
                    .min(self.local.capacity())
                    .min(self.sequence_clocks.capacity()),
            });
        }
        Ok(())
    }
}
