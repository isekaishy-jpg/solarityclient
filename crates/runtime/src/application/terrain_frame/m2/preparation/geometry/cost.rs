//! Geometry hints count known loops without inspecting vertices or simulation items.

use super::super::super::{M2GpuPlacement, M2GpuSource};
use super::GeometryJob;
use solarity_cpu::CostCalibration;

/// Palette/particle/ribbon presence separates materially different kernels. Within
/// each class the loop counts are only scheduling hints, never simulation limits.
#[derive(Default)]
pub(super) struct GeometryCalibration {
    classes: [CostCalibration; 8],
}

impl GeometryCalibration {
    /// No scan of bones, particles or vertices: collection lengths are sufficient.
    pub(super) fn prepare(
        &mut self,
        job: &mut GeometryJob,
        source: &M2GpuSource,
        placement: &M2GpuPlacement,
    ) {
        let class = usize::from(job.palette.pending)
            | (usize::from(!placement.particles.is_empty()) << 1)
            | (usize::from(!placement.ribbons.is_empty()) << 2);
        let bones = if job.palette.pending {
            source.model.animations().bones().len()
        } else {
            0
        };
        let units = placement.particles.iter().fold(
            source.draws.len().saturating_add(bones),
            |units, emitter| {
                units
                    .saturating_add(1)
                    .saturating_add(emitter.simulation.particles().len())
            },
        );
        let units = placement.ribbons.iter().fold(units, |units, trail| {
            units
                .saturating_add(1)
                .saturating_add(trail.sections().len())
        });
        job.cost_class = class;
        job.measurement = self.classes[class].prepare(units.max(1));
    }

    /// Successful measurements are taken once after the owned phase returns.
    pub(super) fn record(&mut self, job: &mut GeometryJob) {
        self.classes[job.cost_class].record(&mut job.measurement);
    }
}
