//! Stock view and Vulkan projection construction.

use glam::{Mat4, Vec3};

use super::{WorldCamera, WorldCameraError, WorldCameraFrame};

impl WorldCamera {
    /// Builds the renderer frame shared by visibility and draw submission.
    ///
    /// The projection uses Vulkan's zero-to-one depth range. World command
    /// recording uses a negative-height viewport for the Y convention, so the
    /// projection must retain positive camera-up in positive clip Y.
    ///
    /// # Errors
    ///
    /// Returns [`WorldCameraError`] when the aspect ratio or authored camera
    /// basis cannot form a finite, non-degenerate projection.
    pub fn frame(self, aspect_ratio: f32) -> Result<WorldCameraFrame, WorldCameraError> {
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
            return Err(WorldCameraError::AspectRatio);
        }
        self.validate()?;

        let direction = self.target() - self.position();
        let forward = direction.normalize();
        let mut up = self.up() - forward * self.up().dot(forward);
        let up_length_squared = up.length_squared();
        if !up_length_squared.is_finite() || up_length_squared <= 1.0e-8 {
            return Err(WorldCameraError::UpDirection);
        }
        up /= up_length_squared.sqrt();
        let right = forward.cross(up).normalize();
        // Recompute up after normalization so all billboard and frustum users
        // consume the exact orthonormal basis used by the view matrix.
        up = right.cross(forward).normalize();

        let view = Mat4::look_at_rh(self.position(), self.target(), up);
        let projection = Mat4::perspective_rh(
            self.vertical_field_of_view_radians(),
            aspect_ratio,
            self.near_clip(),
            self.far_clip(),
        );
        Ok(WorldCameraFrame::new(
            self,
            aspect_ratio,
            forward,
            right,
            up,
            view,
            projection,
        ))
    }

    /// Rejects malformed camera state before glam's projection constructors.
    pub(super) fn validate(self) -> Result<(), WorldCameraError> {
        if !finite_vec3(self.position()) || !finite_vec3(self.target()) || !finite_vec3(self.up()) {
            return Err(WorldCameraError::NonFiniteBasis);
        }
        if (self.target() - self.position()).length_squared() <= 1.0e-8 {
            return Err(WorldCameraError::ViewDirection);
        }
        let field_of_view = self.vertical_field_of_view_radians();
        if !field_of_view.is_finite()
            || field_of_view <= 0.0
            || field_of_view >= core::f32::consts::PI
        {
            return Err(WorldCameraError::FieldOfView);
        }
        if !self.near_clip().is_finite()
            || !self.far_clip().is_finite()
            || self.near_clip() <= 0.0
            || self.far_clip() <= self.near_clip()
        {
            return Err(WorldCameraError::ClipRange);
        }
        if self.subject().is_some_and(|subject| {
            !finite_vec3(subject.orbit_pivot()) || !finite_vec3(subject.position())
        }) {
            return Err(WorldCameraError::NonFiniteSubject);
        }
        Ok(())
    }
}

/// Tests all components without relying on glam's optional assertion feature.
fn finite_vec3(value: Vec3) -> bool {
    value.to_array().into_iter().all(f32::is_finite)
}
