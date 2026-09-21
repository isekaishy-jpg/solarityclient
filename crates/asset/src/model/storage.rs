//! Retained decoded-buffer capacity; encoded decode scratch has a separate lifetime.

/// Vec capacity includes unused retained slots, unlike the public slice length.
#[allow(clippy::ptr_arg)]
pub(super) fn vector_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(size_of::<T>())
}

/// One immutable generation keeps its charge through cache and consumer ownership.
pub(super) struct ModelStorage {
    budget: solarity_cpu::CpuStorageBudget,
    reservation: std::sync::Mutex<solarity_cpu::ByteReservation>,
}
impl ModelStorage {
    pub(super) fn new(
        budget: &solarity_cpu::CpuStorageBudget,
        class: solarity_cpu::CpuStorageClass,
        bytes: usize,
    ) -> Result<Self, solarity_cpu::CpuError> {
        Ok(Self {
            budget: budget.clone(),
            reservation: std::sync::Mutex::new(budget.reserve(
                class,
                solarity_cpu::CpuStorageKind::Result,
                bytes,
            )?),
        })
    }
    /// Promotion moves the existing charge atomically; failure preserves the speculative source.
    pub(super) fn require(&self) -> Result<(), solarity_cpu::CpuError> {
        self.reservation
            .lock()
            .unwrap_or_else(|_| unreachable!("storage metadata cannot panic"))
            .transfer(
                &self.budget,
                solarity_cpu::CpuStorageClass::Required,
                solarity_cpu::CpuStorageKind::Result,
            )
    }
}
impl std::fmt::Debug for ModelStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelStorage").finish_non_exhaustive()
    }
}
