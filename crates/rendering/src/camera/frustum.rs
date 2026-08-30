//! Eye-based stock world frustum and conservative visibility tests.

use glam::Vec3;

use super::{WorldCameraError, WorldCameraFrame, WorldScreenWindow};

/// Six-plane world frustum derived from one validated camera frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrustum {
    position: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    vertical_tangent: f32,
    horizontal_tangent: f32,
    window: WorldScreenWindow,
    near: f32,
    far: f32,
}

impl WorldFrustum {
    /// Builds a complete or portal-clipped frustum from the frame basis.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError::ScreenWindow`] when the normalized bounds
    /// are non-finite, empty, or reversed.
    pub fn new(
        frame: WorldCameraFrame,
        window: WorldScreenWindow,
    ) -> Result<Self, WorldCameraError> {
        if !window.validate() {
            return Err(WorldCameraError::ScreenWindow);
        }
        let camera = frame.camera();
        let vertical_tangent = (camera.vertical_field_of_view_radians() * 0.5).tan();
        Ok(Self {
            position: camera.position(),
            forward: frame.forward(),
            right: frame.right(),
            up: frame.up(),
            vertical_tangent,
            horizontal_tangent: vertical_tangent * frame.aspect_ratio(),
            window,
            near: camera.near_clip(),
            far: camera.far_clip(),
        })
    }

    /// Tests a world-space sphere against all six frustum planes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError::NonFiniteBounds`] for malformed geometry.
    pub fn contains_sphere(self, center: Vec3, radius: f32) -> Result<bool, WorldCameraError> {
        if !finite_vec3(center) || !radius.is_finite() {
            return Err(WorldCameraError::NonFiniteBounds);
        }
        let radius = radius.max(0.0);
        let offset = center - self.position;
        let depth = offset.dot(self.forward);
        if depth + radius < self.near || depth - radius > self.far {
            return Ok(false);
        }
        let outside = |normal: Vec3| offset.dot(normal) > radius * normal.length();
        Ok(!outside(
            self.right - self.forward * (self.window.maximum_x() * self.horizontal_tangent),
        ) && !outside(
            -self.right + self.forward * (self.window.minimum_x() * self.horizontal_tangent),
        ) && !outside(
            self.up - self.forward * (self.window.maximum_y() * self.vertical_tangent),
        ) && !outside(
            -self.up + self.forward * (self.window.minimum_y() * self.vertical_tangent),
        ))
    }

    /// Conservatively tests an oriented box expressed as three half axes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError::NonFiniteBounds`] for malformed geometry.
    pub fn intersects_box(
        self,
        center: Vec3,
        axis_x: Vec3,
        axis_y: Vec3,
        axis_z: Vec3,
    ) -> Result<bool, WorldCameraError> {
        if [center, axis_x, axis_y, axis_z]
            .into_iter()
            .any(|value| !finite_vec3(value))
        {
            return Err(WorldCameraError::NonFiniteBounds);
        }
        let offset = center - self.position;
        let projected_radius = |normal: Vec3| {
            axis_x.dot(normal).abs() + axis_y.dot(normal).abs() + axis_z.dot(normal).abs()
        };
        let outside =
            |normal: Vec3, constant: f32| offset.dot(normal) + constant > projected_radius(normal);
        if outside(-self.forward, self.near) || outside(self.forward, -self.far) {
            return Ok(false);
        }
        Ok(!outside(
            self.right - self.forward * (self.window.maximum_x() * self.horizontal_tangent),
            0.0,
        ) && !outside(
            -self.right + self.forward * (self.window.minimum_x() * self.horizontal_tangent),
            0.0,
        ) && !outside(
            self.up - self.forward * (self.window.maximum_y() * self.vertical_tangent),
            0.0,
        ) && !outside(
            -self.up + self.forward * (self.window.minimum_y() * self.vertical_tangent),
            0.0,
        ))
    }
}

/// Tests all components before visibility math can propagate NaNs.
fn finite_vec3(value: Vec3) -> bool {
    value.to_array().into_iter().all(f32::is_finite)
}
