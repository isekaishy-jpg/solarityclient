//! Raw unit scene bounds, independent of the rider's attached drawing transform.

use glam::{Mat4, Vec3};
use solarity_asset::DecodedM2Model;
use solarity_systems::MovementCollisionBounds;

use super::RuntimeTerrainFrameError;

/// 7370D0 asks GetModel for the mount when present, then uses movement yaw.
#[derive(Clone, Copy)]
pub(super) struct UnitSceneRegistration {
    pub position: Vec3,
    pub bounds: MovementCollisionBounds,
}

impl UnitSceneRegistration {
    /// Retains the selected model's raw world box before drawing changes its basis.
    pub(super) fn new(
        model: &DecodedM2Model,
        transform: Mat4,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let bounds = model.bounds();
        let bounds = MovementCollisionBounds::new(bounds.minimum(), bounds.maximum())
            .and_then(|bounds| bounds.transformed(transform))
            .map_err(crate::application::RuntimeMovementRegistrationError::from)?;
        Ok(Self {
            position: transform.w_axis.truncate(),
            bounds,
        })
    }
}
