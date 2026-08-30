//! Stock implementation responsibility recovered from `Movement_C.cpp`.

use glam::Vec3;
use shipyard::Component;

/// Renderer-independent authoritative world transform.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct WorldTransform {
    position: Vec3,
    orientation: f32,
}

impl WorldTransform {
    /// Creates an authoritative renderer-independent transform.
    #[must_use]
    pub const fn new(position: Vec3, orientation: f32) -> Self {
        Self {
            position,
            orientation,
        }
    }

    /// Returns the authoritative world-space position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns facing orientation in radians.
    #[must_use]
    pub const fn orientation(self) -> f32 {
        self.orientation
    }
}
