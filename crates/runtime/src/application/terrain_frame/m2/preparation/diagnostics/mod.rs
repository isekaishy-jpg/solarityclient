//! Sampled demand/output accounting; no per-placement recorder traffic.

mod placement;
pub(super) use placement::Placement;

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_work_accounting.rs"]
mod tests;

/// Counters live on the frame stack and publish once on a detail frame.
#[derive(Default)]
pub(super) struct Work {
    epoch: u64,
    sources: std::collections::HashSet<u64>,
    visited: u64,
    visible: u64,
    primary_shadow: u64,
    environment_shadow: u64,
    light_owner: u64,
    callback_owner: u64,
    particle_owner: u64,
    palettes: u64,
    batch_hits: u64,
    palettes_without_draws: u64,
    mesh_owners: u64,
    shadow_owners: u64,
    particle_outputs: u64,
    ribbon_outputs: u64,
    cpu_outputs: u64,
    early_rejected: u64,
}

impl Work {
    /// Latches selection once; inner counters require no clocks or atomic loads.
    pub(super) fn new() -> Self {
        Self {
            sources: std::collections::HashSet::new(),
            epoch: if solarity_profiling::detail_enabled() {
                solarity_profiling::generation()
            } else {
                0
            },
            ..Self::default()
        }
    }

    pub(super) fn placement(&mut self) -> Placement<'_> {
        Placement::new(self)
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if self.epoch == 0 || self.epoch != solarity_profiling::generation() {
            return;
        }
        macro_rules! record {
            ($field:ident) => {{
                static SITE: solarity_profiling::Site =
                    solarity_profiling::Site::counter(concat!("m2.work.", stringify!($field)));
                SITE.value_for_generation(self.epoch, self.$field);
            }};
        }
        record!(visited);
        record!(visible);
        record!(primary_shadow);
        record!(environment_shadow);
        record!(light_owner);
        record!(callback_owner);
        record!(particle_owner);
        record!(palettes);
        record!(batch_hits);
        record!(palettes_without_draws);
        record!(mesh_owners);
        record!(shadow_owners);
        record!(particle_outputs);
        record!(ribbon_outputs);
        record!(cpu_outputs);
        record!(early_rejected);
    }
}

impl Work {
    /// Completes output accounting when visible or shadow-only work joins its workers.
    pub(in super::super) fn geometry_outputs(
        &mut self,
        mesh: bool,
        particle: bool,
        ribbon: bool,
        shadow: bool,
    ) {
        if self.epoch == 0 {
            return;
        }
        self.shadow_owners += u64::from(shadow);
        self.mesh_owners += u64::from(mesh);
        self.particle_outputs += u64::from(particle);
        self.ribbon_outputs += u64::from(ribbon);
        self.palettes_without_draws += u64::from(!mesh && !particle && !ribbon && !shadow);
    }
}
