//! Runtime sampling and hierarchy composition for decoded M2 bone tracks.

mod callback;
mod camera;
mod clock;
mod event;
mod light;
mod material;
mod pose;
mod quaternion;
pub(crate) mod sample;
mod sequence_blend;
mod sequence_timer;
mod status;
mod ui_camera;

pub use callback::{M2CallbackSlot, M2QueuedCallback, scan_m2_callbacks};
pub use camera::{M2CameraEffectScale, M2CameraFrameError, sample_m2_camera_frame};
pub use clock::M2AnimationClock;
pub use event::{M2EventTimeWindow, triggered_m2_event_indices};
pub use light::{
    M2SampledLights, sample_m2_directional_lights, sample_m2_lights, sample_m2_lights_into,
    sample_m2_scene_lights_into,
};
pub use material::M2MaterialPose;
pub use pose::{M2BonePose, M2BonePoseOverrides, M2FingerPoseHands};
pub use sequence_blend::M2ModelSequenceBlend;
pub use sequence_timer::{M2ModelSequenceTimer, M2SequenceStartPhase};
pub use status::{M2BonePoseError, M2MaterialPoseError};
pub use ui_camera::{M2UiCameraViewport, sample_m2_ui_camera_frame};
