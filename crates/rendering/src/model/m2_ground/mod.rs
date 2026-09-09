//! Retained unit surface orientation and native M2 model basis selection.

mod normal;
mod transform;

pub use normal::M2GroundNormal;

/// Invalid input at the unit's ground-presentation boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum M2GroundPlacementError {
    /// Surface samples and frame intervals must be finite and time nonnegative.
    #[error("unit ground normal sample is invalid")]
    InvalidSample,
    /// Position, heading, positive scale, and the bounded blend must be finite.
    #[error("unit ground model placement is invalid")]
    InvalidPlacement,
}
