//! Background demand is independent of protected-frame execution eligibility.

/// Service order for finite work on the flexible/bulk allowance.
///
/// Required loading and retirement take bounded turns; speculative work runs
/// only when neither those services nor frame work is ready. These classes do
/// not make archive or decoder calls eligible on protected workers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CpuService {
    /// A selected resource or explicit application operation needs this result.
    Required,
    /// Reclaim detached allocations without starving required loading.
    Retirement,
    /// Prepare optional future demand only from otherwise idle capacity.
    Speculative,
}

impl CpuService {
    /// Decodes only values stored by the typed task service boundary.
    pub(super) fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Required,
            1 => Self::Retirement,
            2 => Self::Speculative,
            _ => unreachable!("service identity contains a CpuService discriminant"),
        }
    }
}
