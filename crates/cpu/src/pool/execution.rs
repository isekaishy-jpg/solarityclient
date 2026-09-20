//! Execution eligibility is independent of changing consumer urgency.

/// Service work always runs on flexible workers. Finite turns perform bounded
/// pure computation; bulk turns may block, enter foreign code or destroy large
/// allocations. Promoting demand never changes this classification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CpuServiceExecution {
    /// A declared bounded turn with no blocking call or indivisible destructor.
    Finite,
    /// Uses one of the execution plan's explicitly limited bulk lanes.
    #[default]
    Bulk,
}
