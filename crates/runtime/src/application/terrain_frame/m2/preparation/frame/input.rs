//! Immutable view inputs survive a pause without retaining unrelated world owners.

use super::super::super::{M2CameraEffectScale, M2TransparentPass, WorldCameraFrame, WorldFrustum};

/// Admission and completion consume the same camera, clocks and lighting values.
#[derive(Clone, Copy)]
pub(super) struct FrameView {
    pub(super) frustum: WorldFrustum,
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

/// The coordinator gets one useful-work turn before reaching a consumption wait.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmissionMode {
    /// Stop before the first unfinished root, without advancing its ordered state.
    Ready,
    /// Independent work has run; wait only at the original palette consumer.
    Complete,
}

/// The placement cursor stays in M2Frame; this state prevents repeat publication
/// of the effect tail when traversal resumes after an unfinished root palette.
pub(super) struct FrameAdmission {
    pub(super) work: super::super::diagnostics::Work,
    pub(super) effects_published: bool,
    pub(super) complete: bool,
}

impl FrameAdmission {
    pub(super) fn new() -> Self {
        Self {
            work: super::super::diagnostics::Work::new(),
            effects_published: false,
            complete: false,
        }
    }
}
