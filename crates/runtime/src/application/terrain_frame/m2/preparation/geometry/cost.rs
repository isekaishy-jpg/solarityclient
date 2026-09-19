//! Geometry hints count known loops without inspecting vertices or simulation items.

use super::super::super::{M2GpuPlacement, M2GpuSource};
use super::GeometryJob;
use solarity_cpu::CostCalibration;

/// Palette/effect presence and visible/shadow demand separate different kernels. Within
/// each class the loop counts are only scheduling hints, never simulation limits.
#[derive(Default)]
pub(super) struct GeometryCalibration {
    classes: [CostCalibration; 32],
}

impl GeometryCalibration {
    /// No scan of bones, particles or vertices: collection lengths are sufficient.
    pub(super) fn prepare(
        &mut self,
        job: &mut GeometryJob,
        source: &M2GpuSource,
        placement: &M2GpuPlacement,
    ) {
        let visible = job.input.is_some_and(|input| input.visible.is_some());
        let shadow = job
            .input
            .is_some_and(|input| input.primary_shadow || input.environment_maps != 0);
        let class = usize::from(job.palette.pending)
            | (usize::from(visible && !placement.particles.is_empty()) << 1)
            | (usize::from(visible && !placement.ribbons.is_empty()) << 2)
            | (usize::from(shadow) << 3)
            | (usize::from(visible) << 4);
        let bones = if job.palette.pending {
            source.model.animations().bones().len()
        } else {
            0
        };
        let materials = source
            .draws
            .len()
            .saturating_mul(usize::from(visible) + usize::from(shadow));
        let units = placement
            .particles
            .iter()
            .take(if visible {
                placement.particles.len()
            } else {
                0
            })
            .fold(materials.saturating_add(bones), |units, emitter| {
                units
                    .saturating_add(1)
                    .saturating_add(emitter.simulation.particles().len())
            });
        let units = placement
            .ribbons
            .iter()
            .take(if visible { placement.ribbons.len() } else { 0 })
            .fold(units, |units, trail| {
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
