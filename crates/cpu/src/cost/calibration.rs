//! Domain-owned work counts calibrate pure kernels without shared hot-path locks.

use super::JobCost;
use std::time::{Duration, Instant};

/// Retained estimate for one homogeneous operation and work-unit definition.
/// The coordinator updates it from returned jobs; workers own only measurements.
#[derive(Default)]
pub struct CostCalibration {
    nanos_per_unit: Option<u64>,
    submissions: u64,
}

impl CostCalibration {
    /// Selects the first eight jobs, then one in 64. Unsampled work reads no clock.
    /// Units describe actual work (for example bones or receivers), never identity.
    pub fn prepare(&mut self, units: usize) -> WorkMeasurement {
        let sample = self.submissions < 8 || self.submissions.is_multiple_of(64);
        self.submissions = self.submissions.saturating_add(1);
        WorkMeasurement {
            units: units as u64,
            cost: self.estimate(units),
            sample: sample && units != 0,
            elapsed: None,
        }
    }

    /// Predicts from completed observations; missing calibration remains explicit.
    #[must_use]
    pub fn estimate(&self, units: usize) -> JobCost {
        self.nanos_per_unit.map_or_else(JobCost::default, |nanos| {
            JobCost::measured(Duration::from_nanos(nanos.saturating_mul(units as u64)))
        })
    }

    /// Incorporates one successful sample, with a 1/8 weight after initialization.
    /// Callers exclude failed/partial kernels; zero units carry no useful rate.
    pub fn observe(&mut self, units: usize, elapsed: Duration) {
        if units == 0 {
            return;
        }
        let nanos = (elapsed.as_nanos() / units as u128).clamp(1, u128::from(u64::MAX)) as u64;
        self.nanos_per_unit = Some(self.nanos_per_unit.map_or(nanos, |previous| {
            ((u128::from(previous) * 7 + u128::from(nanos)) / 8) as u64
        }));
    }

    /// Takes a returned sample exactly once, before its job storage is reused.
    pub fn record(&mut self, measurement: &mut WorkMeasurement) {
        if let Some(elapsed) = measurement.elapsed.take() {
            self.observe(measurement.units as usize, elapsed);
        }
    }

    /// Converts a target quantum into work units, if execution has been calibrated.
    #[must_use]
    pub fn units_for(&self, target: Duration) -> Option<usize> {
        self.nanos_per_unit.map(|nanos| {
            (target.as_nanos() / u128::from(nanos)).clamp(1, usize::MAX as u128) as usize
        })
    }
}

/// Owned job metadata contains no references to the coordinator's calibration.
#[derive(Default)]
pub struct WorkMeasurement {
    units: u64,
    cost: JobCost,
    sample: bool,
    elapsed: Option<Duration>,
}

impl WorkMeasurement {
    /// Returns a frozen hint for this admission, even while calibration advances.
    #[must_use]
    pub fn cost(&self) -> JobCost {
        self.cost
    }

    /// Called outside scheduler locks immediately before the domain kernel.
    #[must_use]
    pub fn start(&self) -> Option<Instant> {
        self.sample.then(Instant::now)
    }

    /// Retains the observation for ordered coordinator reclamation. A failed or
    /// cancelled kernel may omit this call; it cannot corrupt future estimates.
    pub fn finish(&mut self, started: Option<Instant>) {
        self.elapsed = started.map(|started| started.elapsed());
    }
}
