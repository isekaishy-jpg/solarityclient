//! Owned CPU service for crate-internal presentation fixtures.

/// Small owned executor for presentation fixtures that previously selected inline work.
pub(crate) fn executor() -> Result<solarity_cpu::CpuExecutor, solarity_cpu::CpuError> {
    solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        std::num::NonZeroUsize::MIN,
        std::num::NonZeroUsize::new(8).ok_or(solarity_cpu::CpuError::BatchCapacity)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))
}
