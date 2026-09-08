//! Player, world, and vehicle camera policy, transitions, constraints, and shake behavior.
//!
//! This cross-cutting seed coordinates ECS view state with renderer camera
//! projection without placing gameplay policy in either owning state crate.

mod controller;
mod obstruction;
mod transition;
mod types;
mod volume;
mod water_interface;
pub(crate) mod water_segment;

pub use controller::{
    resolve_camera_subject_height, resolve_model_camera_subject_height,
    resolve_mounted_player_camera_pose, resolve_player_camera_pose,
};
pub use obstruction::{
    PlayerCameraObstruction, PlayerCameraObstructionError, PlayerCameraObstructionSettings,
    PlayerCameraSceneQuery, resolve_player_camera_obstruction,
};
pub use transition::PlayerCameraHeightState;
pub use types::{
    CameraSubjectGeometry, CameraSubjectHeight, CameraSubjectHeightError,
    CameraSubjectHeightSource, MountCameraGeometry, MountCameraHeightError,
    PlayerCameraHeightSample, PlayerCameraPose, PlayerCameraPoseError,
};
pub use volume::{
    PlayerCameraVolume, PlayerCameraVolumeError, PlayerCameraVolumeKind,
    PlayerCameraVolumeQueryError, resolve_player_camera_volume,
};
pub use water_interface::{
    PLAYER_CAMERA_WATER_CLEARANCE, PlayerCameraLiquidState, PlayerCameraLiquidStateError,
    PlayerCameraWaterInterfaceError, resolve_player_camera_water_interface,
};
pub use water_segment::{PlayerCameraWaterSegment, PlayerCameraWaterSegmentError};
