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
    pub(super) pending: Option<(SelectedPlacement, super::super::diagnostics::PlacementState)>,
}

impl FrameAdmission {
    pub(super) fn new(storage: solarity_cpu::CpuStorageBudget) -> Self {
        Self {
            storage,
            work: super::super::diagnostics::Work::new(),
            effects_published: false,
            complete: false,
            pending: None,
        }
    }
}

/// Ordered selection survives a bone dependency without ticking or consuming events twice.
pub(super) struct SelectedPlacement {
    pub(super) expired: Vec<crate::application::model_playback::M2ExpiredVariation>,
    pub(super) expired_next: usize,
    pub(super) expired_started: bool,
    pub(super) placement_index: usize,
    pub(super) environment_maps: u8,
    pub(super) publishes_lights: bool,
    pub(super) doodad_scene_active: bool,
    pub(super) placement_fog_color: glam::Vec3,
    pub(super) shadow_opacity: f32,
    pub(super) placement_opacity: f32,
    pub(super) clock: solarity_rendering::M2AnimationClock,
    pub(super) body_pose: Option<crate::application::unit_animation::UnitBodyPoseSample>,
    pub(super) bone_sequences: Vec<(u16, solarity_rendering::M2AnimationClock)>,
    pub(super) finger_pose: Option<(
        solarity_rendering::M2AnimationClock,
        solarity_rendering::M2FingerPoseHands,
    )>,
    pub(super) event_window: solarity_rendering::M2EventTimeWindow,
    pub(super) model_view: glam::Mat4,
    pub(super) instance_identity: usize,
    pub(super) instance_distance: f32,
    pub(super) shadow_admitted: bool,
    pub(super) visible: bool,
    pub(super) needs_palette: bool,
}
