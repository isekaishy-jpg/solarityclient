//! Runtime sampling and hierarchy composition for decoded M2 bone tracks.

mod camera;
mod event;
mod material;
mod pose;
pub(crate) mod sample;
mod status;

pub use camera::{M2CameraFrameError, sample_m2_camera_frame};
pub use event::{M2EventTimeWindow, triggered_m2_event_indices};
pub use material::M2MaterialPose;
pub use pose::{M2AnimationClock, M2BonePose};
pub use status::{M2BonePoseError, M2MaterialPoseError};
