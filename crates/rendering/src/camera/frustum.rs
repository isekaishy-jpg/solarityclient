//! Eye-based stock world frustum and conservative visibility tests.

use glam::Vec3;

use super::{WorldCameraError, WorldCameraFrame, WorldCameraProjection, WorldScreenWindow};

/// Six-plane world frustum derived from one validated camera frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrustum {
    position: Vec3,
    forward: Vec3,
    /// Outward normals and offsets relative to the eye. Keeping this form
    /// avoids losing precision by subtracting large world-space plane terms.
    sides: [(Vec3, f32); 4],
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
        let right = frame.right();
        let up = frame.up();
        let forward = frame.forward();
        let sides = match camera.projection() {
            WorldCameraProjection::Perspective {
                vertical_field_of_view_radians,
            } => {
                let vertical_tangent = (vertical_field_of_view_radians * 0.5).tan();
                let horizontal_tangent = vertical_tangent * frame.aspect_ratio();
                [
                    (
                        right - forward * (window.maximum_x() * horizontal_tangent),
                        0.0,
                    ),
                    (
                        -right + forward * (window.minimum_x() * horizontal_tangent),
                        0.0,
                    ),
                    (up - forward * (window.maximum_y() * vertical_tangent), 0.0),
                    (-up + forward * (window.minimum_y() * vertical_tangent), 0.0),
                ]
            }
            WorldCameraProjection::Orthographic {
                horizontal,
                vertical,
            } => {
                let coordinate = |[minimum, maximum]: [f32; 2], clip: f32| {
                    minimum + (maximum - minimum) * ((clip + 1.0) * 0.5)
                };
                [
                    (right, -coordinate(horizontal, window.maximum_x())),
                    (-right, coordinate(horizontal, window.minimum_x())),
                    (up, -coordinate(vertical, window.maximum_y())),
                    (-up, coordinate(vertical, window.minimum_y())),
                ]
            }
        };
        Ok(Self {
            position: camera.position(),
            forward: frame.forward(),
            sides,
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
        Ok(self
            .sides
            .into_iter()
            .all(|(normal, constant)| offset.dot(normal) + constant <= radius * normal.length()))
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
        Ok(self
            .sides
            .into_iter()
            .all(|(normal, constant)| !outside(normal, constant)))
    }

    /// Rejects a group of axis-aligned boxes only when one plane rejects all
    /// members. Inputs bound the centers and absolute half extents used by
    /// intersects_box, rather than reconstructing a rounded enclosing box.
    pub(crate) fn rejects_box_group(
        self,
        minimum_center: Vec3,
        maximum_center: Vec3,
        maximum_half: Vec3,
    ) -> bool {
        let minimum_offset = minimum_center - self.position;
        let maximum_offset = maximum_center - self.position;
        if !minimum_offset.is_finite() || !maximum_offset.is_finite() {
            return false;
        }
        let maximum_offset_magnitude = minimum_offset.abs().max(maximum_offset.abs());
        let axis_x = Vec3::new(maximum_half.x, 0.0, 0.0);
        let axis_y = Vec3::new(0.0, maximum_half.y, 0.0);
        let axis_z = Vec3::new(0.0, 0.0, maximum_half.z);
        let outside = |normal: Vec3, constant: f32| {
            // Overflow could make an individual box's expression NaN. Keep
            // such groups on the original path instead of hiding its result.
            if !(maximum_offset_magnitude.dot(normal.abs()) + constant.abs()).is_finite() {
                return false;
            }
            let offset = Vec3::select(normal.cmpge(Vec3::ZERO), minimum_offset, maximum_offset);
            let radius =
                axis_x.dot(normal).abs() + axis_y.dot(normal).abs() + axis_z.dot(normal).abs();
            // Each operation is the same monotone f32 operation as the member
            // test: the selected offset gives a lower bound and the largest
            // half axes give an upper bound. No epsilon or new plane math.
            radius.is_finite() && offset.dot(normal) + constant > radius
        };
        outside(-self.forward, self.near)
            || outside(self.forward, -self.far)
            || self
                .sides
                .into_iter()
                .any(|(normal, constant)| outside(normal, constant))
    }
}

/// Tests all components before visibility math can propagate NaNs.
fn finite_vec3(value: Vec3) -> bool {
    value.to_array().into_iter().all(f32::is_finite)
}
