//! Per-model state crosses workers with its immutable generation and budgeted outputs.

use super::super::super::{
    M2BonePose, M2MaterialPose, M2ParticlePlacement, M2RibbonTrail, M2TransparentElement,
    RuntimeTerrainFrameError,
};
use solarity_rendering::{
    M2ParticlePreparedDraw, M2ParticleRenderVertex, M2PreparedDraw, M2RibbonPreparedDraw,
    M2RibbonRenderVertex,
};

use super::GeometryInput;

/// Reuses frame-local scratch and temporarily owns each admitted model's effect state.
#[derive(Default)]
pub(super) struct GeometryJob {
    pub(super) reuse_identity: Option<super::reuse::GeometryReuseIdentity>,
    pub(super) next_reuse: Option<usize>,
    pub(super) measurement: solarity_cpu::WorkMeasurement,
    pub(super) cost_class: usize,
    pub(super) input: Option<GeometryInput>,
    pub(super) context: Option<GeometryContext>,
    pub(super) owns_effects: bool,
    pub(super) publishes_palette: bool,
    pub(super) pose: M2BonePose,
    pub(super) palette: super::palette::PaletteInput,
    pub(super) material_poses: Vec<Option<M2MaterialPose>>,
    pub(super) particles: Vec<M2ParticlePlacement>,
    pub(super) ribbons: Vec<M2RibbonTrail>,
    pub(super) shadow_draws: solarity_cpu::CpuBuffer<M2PreparedDraw>,
    pub(super) visible_draws: solarity_cpu::CpuBuffer<M2PreparedDraw>,
    pub(super) transparent_elements: solarity_cpu::CpuBuffer<M2TransparentElement>,
    pub(super) particle_vertices: solarity_cpu::CpuBuffer<M2ParticleRenderVertex>,
    pub(super) particle_indices: solarity_cpu::CpuBuffer<u32>,
    pub(super) particle_sort_indices: solarity_cpu::CpuScratch<usize>,
    pub(super) particle_draws: solarity_cpu::CpuBuffer<M2ParticlePreparedDraw>,
    pub(super) ribbon_vertices: solarity_cpu::CpuBuffer<M2RibbonRenderVertex>,
    pub(super) ribbon_draws: solarity_cpu::CpuBuffer<M2RibbonPreparedDraw>,
    pub(super) particle_vertex_capacity: usize,
    pub(super) particle_index_capacity: usize,
    pub(super) recoverable_errors: Vec<String>,
    pub(super) result: Option<Result<(), RuntimeTerrainFrameError>>,
}

/// Immutable generation and camera inputs own every worker dependency.
pub(super) struct GeometryContext {
    pub(super) source: super::super::super::M2GpuSource,
    pub(super) camera: solarity_rendering::WorldCameraFrame,
    pub(super) effect_scale: solarity_rendering::M2CameraEffectScale,
    pub(super) twinkle: std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
}

impl GeometryJob {
    /// Executes after this model's ancestry, clock and effect state are owned.
    pub(super) fn execute(&mut self, job_context: &solarity_cpu::JobContext<'_>) {
        let started = self.measurement.start();
        let context = self
            .context
            .take()
            .unwrap_or_else(|| unreachable!("admitted geometry owns its context"));
        self.result = Some(self.prepare(&context, job_context));
        // Resource pins need only survive preparation; placement/source and
        // published GPU-frame leases own their later lifetimes. Calibration
        // includes returning those pins, which is part of this worker's kernel.
        drop(context);
        if self.result.as_ref().is_some_and(Result::is_ok) {
            self.measurement.finish(started);
        }
    }

    /// Clears output lengths while retaining storage for the next visible model.
    pub(super) fn reset(&mut self) {
        self.publishes_palette = false;
        self.shadow_draws.clear();
        self.visible_draws.clear();
        self.transparent_elements.clear();
        self.particle_vertices.clear();
        self.particle_indices.clear();
        self.particle_draws.clear();
        self.ribbon_vertices.clear();
        self.ribbon_draws.clear();
        self.particle_vertex_capacity = 0;
        self.particle_index_capacity = 0;
        self.recoverable_errors.clear();
        self.result = None;
    }
}
