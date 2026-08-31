//! Runtime sampling and hierarchy composition for decoded M2 bone tracks.

mod material;
mod pose;
mod sample;
mod status;

pub use material::M2MaterialPose;
pub use pose::{M2AnimationClock, M2BonePose};
pub use status::{M2BonePoseError, M2MaterialPoseError};
