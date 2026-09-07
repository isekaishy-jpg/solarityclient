//! Applies movement updates, interpolation, and trajectory transitions.
//!
//! This behavior is separated from movement components by the stock
//! `Movement.cpp`, `Movement_C.cpp`, and `MovementShared.cpp` family.

mod airborne;
mod animation;
mod body_orientation;
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
mod remote;
mod stance;
mod transport;
mod unit_animation;

pub use airborne::{
    MovementFallAdvance, MovementFallAdvanceError, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallPhase, MovementFallSnapshot,
    MovementFallState,
};
pub use animation::{UnitModelAnimation, resolve_unit_model_animation};
pub use body_orientation::{
    UnitBodyOrientation, UnitBodyOrientationInput, UnitBodyOrientationSample,
};
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
pub use movement_shared::{
    UnitLocomotionAnimation, resolve_unit_locomotion_animation, resolve_unit_movement_speed,
};
pub use path::{
    MovementPath, MovementPathError, MovementPathMode, MovementPathRequest, MovementPathSample,
    MovementSpline, MovementSplineDefinition, MovementSplineError, MovementSplineFacing,
    PreparedMovementPath, advance_world_movement_spline, advance_world_movement_splines,
    set_world_movement_spline,
};
pub use player::{WorldEntryGroundContact, WorldEntryGroundContactError};
pub use remote::{
    RemoteMovementAdmission, RemoteMovementBlend, RemoteMovementClock, RemoteMovementPose,
    RemoteMovementReceipt,
};
pub use stance::{
    UnitPrimaryAnimationCompletion, UnitStandAnimationDecision,
    resolve_unit_primary_animation_completion, resolve_unit_stand_animation,
    resolve_unit_stand_completion, resolve_unit_stand_transition,
};
pub use transport::{
    MovementTransportChange, MovementTransportFrame, MovementTransportFrameError,
    MovementTransportVolume, TransportRoute, TransportRouteClock, TransportRouteError,
    TransportRouteEvent, TransportRouteMotion, TransportRouteNode, TransportRoutePhysics,
    TransportRoutePhysicsError, TransportRouteSample,
};
pub use unit_animation::{
    UnitMovementAnimationDecision, resolve_unit_airborne_animation, resolve_unit_landing_animation,
    resolve_unit_movement_animation_completion, resolve_unit_turn_animation,
    unit_movement_is_airborne,
};
