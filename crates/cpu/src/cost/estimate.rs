//! Coarse execution bins keep admission linear and ties in readiness order.

use std::time::Duration;

/// Estimated kernel time, independent of admission priority and worker eligibility.
/// Zero means uncalibrated; unknown work shares the middle bin rather than being
/// mistaken for free work. The thresholds are scheduling policy, not deadlines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JobCost {
    nanos: u64,
}

impl JobCost {
    /// Saturates large hints; no estimate can reject otherwise valid work.
    #[must_use]
    pub fn measured(duration: Duration) -> Self {
        Self {
            nanos: duration.as_nanos().clamp(1, u128::from(u64::MAX)) as u64,
        }
    }

    /// Returns the calibrated duration, or absence before a usable observation.
    #[must_use]
    pub fn duration(self) -> Option<Duration> {
        (self.nanos != 0).then(|| Duration::from_nanos(self.nanos))
    }

    /// Constant-size classification avoids a per-frame owner sort. Larger bins
    /// run first; urgency is selected independently before any cost comparison.
    pub(crate) fn bin(self) -> usize {
        match self.nanos {
            1..50_000 => 0,
            200_000.. => 2,
            _ => 1,
        }
    }
}
