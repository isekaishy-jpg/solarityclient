//! Player, world, and vehicle camera policy, transitions, constraints, and shake behavior.
//!
//! This cross-cutting seed coordinates ECS view state with renderer camera
//! projection without placing gameplay policy in either owning state crate.

mod controller;
mod obstruction;
mod transition;
mod types;
mod water;

pub use controller::{
    resolve_camera_subject_height, resolve_model_camera_subject_height, resolve_player_camera_pose,
};
pub use obstruction::{PlayerCameraObstructionError, resolve_player_camera_obstruction};
pub use types::{
    CameraSubjectGeometry, CameraSubjectHeight, CameraSubjectHeightError,
    CameraSubjectHeightSource, PlayerCameraPose, PlayerCameraPoseError,
};
pub use water::{PlayerCameraWaterError, resolve_player_camera_water_collision};
