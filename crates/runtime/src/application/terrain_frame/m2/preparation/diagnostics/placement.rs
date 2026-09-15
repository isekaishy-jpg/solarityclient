//! A placement transaction accounts early rejection as well as completed output.

use super::Work;

pub(in super::super) struct Placement<'a> {
    work: &'a mut Work,
    pub(in super::super) admitted: bool,
    pub(in super::super) visible: bool,
    pub(in super::super) primary_shadow: bool,
    pub(in super::super) environment_shadow: bool,
    pub(in super::super) light_owner: bool,
    pub(in super::super) callback_owner: bool,
    pub(in super::super) particle_owner: bool,
    pub(in super::super) palette: bool,
    pub(in super::super) batch_hit: bool,
    pub(in super::super) mesh_output: bool,
    pub(in super::super) geometry_pending: bool,
    pub(in super::super) shadow_output: bool,
    pub(in super::super) particle_output: bool,
    pub(in super::super) ribbon_output: bool,
    pub(in super::super) cpu_output: bool,
}

impl<'a> Placement<'a> {
    pub(in super::super) fn new(work: &'a mut Work) -> Self {
        Self {
            work,
            admitted: false,
            visible: false,
            primary_shadow: false,
            environment_shadow: false,
            light_owner: false,
            callback_owner: false,
            particle_owner: false,
            palette: false,
            batch_hit: false,
            mesh_output: false,
            geometry_pending: false,
            shadow_output: false,
            particle_output: false,
            ribbon_output: false,
            cpu_output: false,
        }
    }
}

impl Placement<'_> {
    /// Publishes one source dictionary entry per sampled frame, never per placement.
    pub(in super::super) fn source(&mut self, index: u64, path: &str) {
        if self.work.epoch != 0 && self.work.sources.insert(index) {
            solarity_profiling::TraceContext::capture().asset(index, path);
        }
    }
}

impl Drop for Placement<'_> {
    fn drop(&mut self) {
        if self.work.epoch == 0 {
            return;
        }
        let reasons = u64::from(self.admitted)
            | (u64::from(self.visible) << 1)
            | (u64::from(self.primary_shadow) << 2)
            | (u64::from(self.environment_shadow) << 3)
            | (u64::from(self.light_owner) << 4)
            | (u64::from(self.callback_owner) << 5)
            | (u64::from(self.particle_owner) << 6)
            | (u64::from(self.palette) << 7)
            | (u64::from(self.batch_hit) << 8)
            | (u64::from(self.shadow_output) << 9)
            | (u64::from(self.cpu_output) << 10)
            | (u64::from(self.geometry_pending) << 11);
        solarity_profiling::TraceContext::capture().value("m2.requirements", 0, reasons, 1);
        self.work.visited += 1;
        self.work.early_rejected += u64::from(!self.admitted);
        self.work.visible += u64::from(self.visible);
        self.work.primary_shadow += u64::from(self.primary_shadow);
        self.work.environment_shadow += u64::from(self.environment_shadow);
        self.work.light_owner += u64::from(self.light_owner);
        self.work.callback_owner += u64::from(self.callback_owner);
        self.work.particle_owner += u64::from(self.particle_owner);
        self.work.palettes += u64::from(self.palette);
        self.work.batch_hits += u64::from(self.batch_hit);
        self.work.palettes_without_draws += u64::from(
            self.palette
                && !self.geometry_pending
                && !self.mesh_output
                && !self.shadow_output
                && !self.particle_output
                && !self.ribbon_output,
        );
        self.work.mesh_owners += u64::from(self.mesh_output);
        self.work.shadow_owners += u64::from(self.shadow_output);
        self.work.particle_outputs += u64::from(self.particle_output);
        self.work.ribbon_outputs += u64::from(self.ribbon_output);
        self.work.cpu_outputs += u64::from(self.cpu_output);
    }
}
