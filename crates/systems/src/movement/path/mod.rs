//! Native Path.cpp geometry and distance-weighted parameter selection.

mod geometry;
mod placement;
mod preparation;
mod spline;
mod world;

pub use world::{
    advance_world_movement_spline, advance_world_movement_splines, set_world_movement_spline,
};

pub use geometry::{MovementPath, MovementPathError, MovementPathMode, MovementPathSample};
pub use placement::MovementSplineTarget;
pub use preparation::{MovementPathRequest, PreparedMovementPath};
pub use spline::{
    MovementSpline, MovementSplineDefinition, MovementSplineError, MovementSplineFacing,
};
