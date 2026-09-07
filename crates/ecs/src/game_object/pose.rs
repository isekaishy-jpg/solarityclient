//! Behavior-owned transport pose, independent of replicated movement inputs.

use glam::Mat4;
use shipyard::Component;

/// Native transport matrix and canonical packed quaternion (`0x007134A0`).
///
/// The full unscaled matrix drives model placement and passenger positions.
/// The separately packed rotation drives passenger quaternion composition.
/// Placement admission validates these raw behavior outputs before use.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct GameObjectAnimatedPose {
    matrix: Mat4,
    packed_rotation: u64,
}

impl GameObjectAnimatedPose {
    /// Retains both native representations without rebuilding either from the other.
    #[must_use]
    pub const fn new(matrix: Mat4, packed_rotation: u64) -> Self {
        Self {
            matrix,
            packed_rotation,
        }
    }

    /// Returns the unscaled world matrix in server Z-up coordinates.
    #[must_use]
    pub const fn matrix(self) -> Mat4 {
        self.matrix
    }

    /// Returns the canonical positive-W 22/21/21-bit rotation.
    #[must_use]
    pub const fn packed_rotation(self) -> u64 {
        self.packed_rotation
    }
}
