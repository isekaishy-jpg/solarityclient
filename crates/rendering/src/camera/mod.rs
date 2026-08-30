//! View and projection calculation plus renderer-facing camera uniform ownership.
//!
//! This is the graphics side of stock `Camera.cpp`; player and vehicle camera
//! policy remains in `systems`, while persistent view state remains in `ecs`.

mod camera_source;
mod frustum;
mod status;
mod types;

pub use frustum::WorldFrustum;
pub use status::WorldCameraError;
pub use types::{
    WORLD_DEPTH_MAXIMUM, WORLD_DEPTH_MINIMUM, WORLD_NEAR_CLIP,
    WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS, WorldCamera, WorldCameraFrame, WorldScreenWindow,
};
