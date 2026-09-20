//! Finite spatial groups own inline inputs and return errors at each model's slot.

use super::input::{SpatialView, StaticAdmission, StaticAdmissionInput};
use crate::application::terrain_frame::m2::RuntimeTerrainFrameError;
use solarity_cpu::WorkMeasurement;

pub(super) const MAX_ENTRIES: usize = 64;
pub(super) type Results = [Option<Result<StaticAdmission, RuntimeTerrainFrameError>>; MAX_ENTRIES];

/// Inline arrays keep the complete job working set in executor-accounted cells.
/// A malformed later entry must not report its error before earlier callbacks.
pub(super) struct SpatialJob {
    pub view: SpatialView,
    pub inputs: [Option<StaticAdmissionInput>; MAX_ENTRIES],
    pub results: Results,
    pub measurement: WorkMeasurement,
    pub first_placement: usize,
    pub count: usize,
}

impl SpatialJob {
    /// Publishes per-owner failures in order; required predicates are not cancelled.
    pub fn run(&mut self, context: &solarity_cpu::JobContext<'_>) -> solarity_cpu::JobOutcome {
        context.diagnostic_value("m2.spatial.entries", self.count as u64);
        self.execute();
        solarity_cpu::JobOutcome::Succeeded
    }

    /// Starts an empty reusable group without allocating model-local scratch.
    pub fn new(view: SpatialView) -> Self {
        Self {
            view,
            inputs: [None; MAX_ENTRIES],
            results: std::array::from_fn(|_| None),
            measurement: WorkMeasurement::default(),
            first_placement: 0,
            count: 0,
        }
    }

    /// Pure native predicates can finish out of order; no worker calls gameplay.
    pub fn execute(&mut self) {
        let mut profile = solarity_profiling::profile!("m2.static_admission.execute");
        profile.trace_owner(self.first_placement as u64 + 1, self.count as u64);
        let started = self.measurement.start();
        let mut successful = true;
        for (input, result) in self.inputs[..self.count].iter().zip(&mut self.results) {
            let value = input
                .unwrap_or_else(|| unreachable!("admitted entries have inputs"))
                .evaluate(self.view);
            successful &= value.is_ok();
            *result = Some(value);
        }
        if successful {
            self.measurement.finish(started);
        }
    }
}
