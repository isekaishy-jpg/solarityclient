//! Native swimming trajectories and collision response.

mod collision;
mod immersion;
mod trajectory;

pub use collision::{
    MovementSwimAdvance, MovementSwimAdvanceError, MovementSwimGeometry, MovementSwimInterval,
};

pub use immersion::{
    MovementSwimImmersion, MovementSwimImmersionError, MovementSwimImmersionUpdate,
    MovementSwimTransition,
};
pub use trajectory::{MovementSwimSample, MovementSwimTrajectory, MovementSwimTrajectoryError};
