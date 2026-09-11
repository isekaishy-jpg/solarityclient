//! Raw unit scene bounds, independent of the rider's attached drawing transform.

#[cfg(test)]
#[path = "../../../../tests/application/model_scene_sphere.rs"]
mod tests;

use glam::{Mat4, Vec3};
use solarity_asset::DecodedM2Model;
use solarity_systems::MovementCollisionBounds;

use super::RuntimeTerrainFrameError;

/// 7370D0 asks GetModel for the mount when present, then uses movement yaw.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct UnitSceneRegistration {
    pub position: Vec3,
    pub bounds: MovementCollisionBounds,
    pub sphere: (Vec3, f32),
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
            sphere: Self::model_sphere(model, transform),
        })
    }

    /// 4F5E80 supplies the authored sphere; 780240 uses only the first axis
    /// length and collapses radii at or below 0.001 to the placement origin.
    pub(super) fn model_sphere(model: &DecodedM2Model, transform: Mat4) -> (Vec3, f32) {
        let bounds = model.bounds();
        if bounds.sphere_radius() <= 0.001 {
            return (transform.w_axis.truncate(), 0.);
        }
        let center = ((bounds.minimum().as_dvec3() + bounds.maximum().as_dvec3()) * 0.5).as_vec3();
        let matrix = transform.to_cols_array();
        let point = center.as_dvec3();
        let center = Vec3::from_array(std::array::from_fn(|axis| {
            (((point.z * f64::from(matrix[8 + axis]) + point.y * f64::from(matrix[4 + axis]))
                + point.x * f64::from(matrix[axis]))
                + f64::from(matrix[12 + axis])) as f32
        }));
        let axis = transform.x_axis.truncate().as_dvec3();
        let scale = ((axis.x * axis.x + axis.y * axis.y) + axis.z * axis.z).sqrt() as f32;
        (center, scale * bounds.sphere_radius())
    }
}
