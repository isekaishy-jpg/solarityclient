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

/// Reuses a test-only service for authored GPU admission across scene fixtures.
/// Production always supplies the application executor and its native wait owner.
pub(crate) fn gpu_preparation(
    renderer: &mut solarity_rendering::VulkanRenderer,
) -> solarity_rendering::GpuPreparation<'_> {
    static CPU: std::sync::OnceLock<solarity_cpu::CpuExecutor> = std::sync::OnceLock::new();
    let cpu = CPU
        .get_or_init(|| executor().unwrap_or_else(|error| panic!("fixture CPU service: {error}")));
    solarity_rendering::GpuPreparation::offline(renderer, cpu)
}
