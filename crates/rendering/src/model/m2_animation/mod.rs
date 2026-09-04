//! Runtime sampling and hierarchy composition for decoded M2 bone tracks.

mod camera;
mod event;
mod light;
mod material;
mod pose;
pub(crate) mod sample;
mod status;

pub use camera::{M2CameraEffectScale, M2CameraFrameError, sample_m2_camera_frame};
pub use event::{M2EventTimeWindow, triggered_m2_event_indices};
pub use light::{
    M2SampledLights, sample_m2_directional_lights, sample_m2_lights, sample_m2_lights_into,
};
pub use material::M2MaterialPose;
pub use pose::{M2AnimationClock, M2BonePose, M2FingerPoseHands};
pub use status::{M2BonePoseError, M2MaterialPoseError};
