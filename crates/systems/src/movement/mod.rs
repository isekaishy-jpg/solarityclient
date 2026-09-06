//! Applies movement updates, interpolation, and trajectory transitions.
//!
//! This behavior is separated from movement components by the stock
//! `Movement.cpp`, `Movement_C.cpp`, and `MovementShared.cpp` family.

mod airborne;
mod animation;
mod clock;
mod contact;
mod fall;
mod geometry;
mod ground_trajectory;
mod grounded;
mod interval_bounds;
mod movement_shared;
mod movement_source;
mod path;
mod player;

pub use airborne::{
    MovementFallAdvance, MovementFallAdvanceError, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallPhase, MovementFallSnapshot,
    MovementFallState,
};
pub use animation::{UnitModelAnimation, resolve_unit_model_animation};
pub use contact::{MovementFallContact, MovementFallContactError, MovementFallContactQuery};
pub use fall::{MovementFallCrossing, MovementFallError, MovementFallMode, MovementFallTrajectory};
pub use geometry::MovementGeometry;
pub use ground_trajectory::{
    MovementGroundSample, MovementGroundTrajectory, MovementGroundTrajectoryError,
    MovementYawTrajectory, MovementYawTrajectoryError,
};
pub use grounded::{
    MovementFallAdmission, MovementGroundAdvance, MovementGroundAdvanceError,
    MovementGroundContinuation, MovementGroundInterval, MovementGroundProfile,
    MovementGroundSnapshot, MovementGroundState,
};
pub use interval_bounds::{
    MovementIntervalBounds, MovementIntervalBoundsError, MovementIntervalMode,
    MovementIntervalRequest,
};
pub use movement_shared::{UnitLocomotionAnimation, resolve_unit_locomotion_animation};
pub use player::{WorldEntryGroundContact, WorldEntryGroundContactError};
