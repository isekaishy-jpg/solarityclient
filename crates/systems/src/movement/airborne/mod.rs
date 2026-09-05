//! Native fall interval state and its collision-driven continuation.

mod advance;
mod state;

pub use state::{
    MovementFallAdvance, MovementFallAdvanceError, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallPhase, MovementFallSnapshot,
    MovementFallState,
};
