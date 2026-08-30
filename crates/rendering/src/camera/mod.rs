//! View and projection calculation plus renderer-facing camera uniform ownership.
//!
//! This is the graphics side of stock `Camera.cpp`; player and vehicle camera
//! policy remains in `systems`, while persistent view state remains in `ecs`.

mod camera_source;
