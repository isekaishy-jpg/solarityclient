//! Immutable view inputs survive a pause without retaining unrelated world owners.

use super::super::super::{M2CameraEffectScale, M2TransparentPass, WorldCameraFrame, WorldFrustum};

/// Admission and completion consume the same camera, clocks and lighting values.
#[derive(Clone, Copy)]
pub(super) struct FrameView {
    pub(super) frustum: WorldFrustum,
    pub(super) liquid_clipping_enabled: bool,
    pub(super) camera: WorldCameraFrame,
    pub(super) first_transparent_pass: M2TransparentPass,
    pub(super) fog_color: glam::Vec3,
    pub(super) animation_time_ms: f32,
    pub(super) effect_scale: M2CameraEffectScale,
    pub(super) world_lighting: Option<(
        solarity_rendering::M2SceneUniform,
        solarity_rendering::M2DirectionalLight,
    )>,
    pub(super) shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
}

/// The placement cursor stays in M2Frame; this state prevents repeat publication
/// of the effect tail when traversal resumes after an unfinished root palette.
pub(super) struct FrameAdmission {
    pub(super) storage: solarity_cpu::CpuStorageBudget,
    pub(super) work: super::super::diagnostics::Work,
    pub(super) effects_published: bool,
    pub(super) complete: bool,
}

impl FrameAdmission {
    pub(super) fn new(storage: solarity_cpu::CpuStorageBudget) -> Self {
        Self {
            storage,
            work: super::super::diagnostics::Work::new(),
            effects_published: false,
            complete: false,
        }
    }
}
