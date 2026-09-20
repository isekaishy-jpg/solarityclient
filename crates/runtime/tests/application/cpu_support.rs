//! Owned CPU service for crate-internal presentation fixtures.

#[path = "frame_continuation_support.rs"]
pub(crate) mod continuation_support;

/// Small owned executor for presentation fixtures that previously selected inline work.
pub(crate) fn executor() -> Result<solarity_cpu::CpuExecutor, solarity_cpu::CpuError> {
    solarity_cpu::CpuExecutor::new(solarity_cpu::CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        std::num::NonZeroUsize::new(8).ok_or(solarity_cpu::CpuError::BatchCapacity)?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))
}
