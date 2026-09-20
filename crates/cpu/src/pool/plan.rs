//! Validated execution eligibility; runtime owns machine-specific policy.

use super::CpuError;
use std::num::NonZeroUsize;

/// All compute-heavy service runs within flexible capacity, never in an extra
/// hidden pool. A service turn may be indivisible; protected workers remain free
/// of that work. Counts describe threads, not exclusively owned physical cores.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuExecutionPlan {
    total: NonZeroUsize,
    protected: usize,
    flexible: NonZeroUsize,
    service_reserve: NonZeroUsize,
    bulk_limit: NonZeroUsize,
}

impl CpuExecutionPlan {
    /// Resolves an explicit split and the maximum concurrent service calls.
    /// Reserved flexible workers prefer required/retirement service; remaining
    /// flexible workers prefer ready frames and assist service when frames idle.
    /// # Errors
    /// Rejects overflow, zero flexible/service/bulk capacity, or a reserve above
    /// flexible capacity or a bulk limit above flexible capacity.
    pub fn new(
        protected: usize,
        flexible: usize,
        service_reserve: usize,
        bulk_limit: usize,
    ) -> Result<Self, CpuError> {
        let invalid = || CpuError::InvalidExecutionPlan;
        let flexible = NonZeroUsize::new(flexible).ok_or_else(invalid)?;
        let service_reserve = NonZeroUsize::new(service_reserve).ok_or_else(invalid)?;
        let bulk_limit = NonZeroUsize::new(bulk_limit).ok_or_else(invalid)?;
        if service_reserve > flexible || bulk_limit > flexible {
            return Err(invalid());
        }
        let total = protected
            .checked_add(flexible.get())
            .and_then(NonZeroUsize::new)
            .ok_or_else(invalid)?;
        Ok(Self {
            total,
            protected,
            flexible,
            service_reserve,
            bulk_limit,
        })
    }

    /// Total compute threads, including every flexible/bulk lane.
    #[must_use]
    pub const fn worker_count(self) -> NonZeroUsize {
        self.total
    }
    /// Threads that never execute ordinary background or indivisible bulk work.
    #[must_use]
    pub const fn protected_workers(self) -> usize {
        self.protected
    }
    /// Threads eligible for both frame and service work within the same budget.
    #[must_use]
    pub const fn flexible_workers(self) -> NonZeroUsize {
        self.flexible
    }
    /// Flexible workers that prefer required service at their next boundary.
    #[must_use]
    pub const fn service_reserve(self) -> NonZeroUsize {
        self.service_reserve
    }
    /// Maximum simultaneously executing bulk calls across flexible workers.
    /// Finite nonblocking service turns do not consume this allowance.
    #[must_use]
    pub const fn bulk_limit(self) -> NonZeroUsize {
        self.bulk_limit
    }
}
