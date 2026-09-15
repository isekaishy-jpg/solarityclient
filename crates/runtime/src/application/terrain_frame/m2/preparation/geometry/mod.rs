//! Owned visible work joins after ordered animation, attachment and shadow publication.

mod output;
mod palette;
mod prepare;
mod publication;
mod ribbons;

pub(in super::super) use ribbons::advance_ribbons;

use super::super::{
    M2BonePose, M2MaterialPose, M2ParticlePlacement, M2RibbonTrail, M2TransparentElement,
    RuntimeTerrainFrameError,
};
use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    M2AnimationClock, M2LiquidPasses, M2ParticleColorReplacement, M2ParticlePreparedDraw,
    M2ParticleRenderVertex, M2PreparedDraw, M2RibbonPreparedDraw, M2RibbonRenderVertex,
    M2SceneLightBank,
};

/// Inputs are copied only after all native callbacks and ancestry requirements resolve.
#[derive(Clone, Copy)]
pub(in super::super) struct GeometryInput {
    pub placement_index: usize,
    pub trace: solarity_profiling::TraceContext,
    pub source_index: usize,
    pub clock: M2AnimationClock,
    pub effect_delta_seconds: f32,
    pub transform: Mat4,
    pub model_view: Mat4,
    pub instance_color: Vec4,
    pub placement_fog_color: Vec3,
    pub light_bank: M2SceneLightBank,
    pub scene_index: Option<u32>,
    pub bone_offset: u32,
    pub effect_retiring: bool,
    pub particle_liquid: M2LiquidPasses,
    pub model_liquid: M2LiquidPasses,
    pub instance_distance: f32,
    pub instance_identity: usize,
    pub model_distance_sort: bool,
    pub particle_colors: Option<M2ParticleColorReplacement>,
    pub has_shadow_bones: bool,
}

/// Reuses frame-local scratch and temporarily owns each admitted model's effect state.
#[derive(Default)]
struct GeometryJob {
    input: Option<GeometryInput>,
    context: Option<GeometryContext>,
    owns_effects: bool,
    pose: M2BonePose,
    palette: palette::PaletteInput,
    material_poses: Vec<Option<M2MaterialPose>>,
    particles: Vec<M2ParticlePlacement>,
    ribbons: Vec<M2RibbonTrail>,
    visible_draws: Vec<M2PreparedDraw>,
    transparent_elements: Vec<M2TransparentElement>,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    particle_sort_indices: Vec<usize>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    particle_vertex_capacity: usize,
    particle_index_capacity: usize,
    recoverable_errors: Vec<String>,
    result: Option<Result<(), RuntimeTerrainFrameError>>,
}

/// Retains only the current admitted job count, never historical model generations.
pub(in super::super::super) struct GeometryBatch {
    jobs: Vec<GeometryJob>,
    active: usize,
    pending: solarity_cpu::FrameBatch<GeometryJob>,
    handles: Vec<solarity_cpu::FrameJob<GeometryJob>>,
    submitted: bool,
}

/// Immutable generation and camera inputs own every worker dependency.
struct GeometryContext {
    source: super::super::M2GpuSource,
    camera: solarity_rendering::WorldCameraFrame,
    effect_scale: solarity_rendering::M2CameraEffectScale,
    twinkle: std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
}

impl Default for GeometryBatch {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            active: 0,
            pending: solarity_cpu::FrameBatch::new(GeometryJob::execute),
            handles: Vec::new(),
            submitted: false,
        }
    }
}

impl GeometryJob {
    /// Executes after this model's ancestry, clock and effect state are owned.
    fn execute(&mut self) {
        let context = self
            .context
            .take()
            .unwrap_or_else(|| unreachable!("admitted geometry owns its context"));
        self.result = Some(self.prepare(
            &context.source,
            context.camera,
            context.effect_scale,
            &context.twinkle,
        ));
        // Resource pins need only survive preparation; placement/source and
        // published GPU-frame leases own their later lifetimes.
    }

    /// Clears output lengths while retaining storage for the next visible model.
    fn reset(&mut self) {
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

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_geometry.rs"]
mod tests;
