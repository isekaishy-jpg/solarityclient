//! Stock view and Vulkan projection construction.

use glam::{Mat4, Vec3};

use super::{WorldCamera, WorldCameraError, WorldCameraFrame, WorldCameraProjection};

impl WorldCameraFrame {
    /// Returns the world-space bottom-right far corner used by terrain demand.
    ///
    /// The far plane has clip Z=1 in both stock's projection and Vulkan. Each
    /// inverse matrix is resolved independently before composing the transform.
    #[must_use]
    pub fn terrain_streaming_corner(self) -> Vec3 {
        let inverse = self.view().inverse() * self.projection().inverse();
        inverse.project_point3(Vec3::new(1.0, -1.0, 1.0))
    }
}

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
        let projection = match self.projection() {
            WorldCameraProjection::Perspective {
                vertical_field_of_view_radians,
            } => Mat4::perspective_rh(
                vertical_field_of_view_radians,
                aspect_ratio,
                self.near_clip(),
                self.far_clip(),
            ),
            WorldCameraProjection::Orthographic {
                horizontal,
                vertical,
            } => Mat4::orthographic_rh(
                horizontal[0],
                horizontal[1],
                vertical[0],
                vertical[1],
                self.near_clip(),
                self.far_clip(),
            ),
        };
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
        match self.projection() {
            WorldCameraProjection::Perspective {
                vertical_field_of_view_radians,
            } => {
                if !vertical_field_of_view_radians.is_finite()
                    || vertical_field_of_view_radians <= 0.0
                    || vertical_field_of_view_radians >= core::f32::consts::PI
                {
                    return Err(WorldCameraError::FieldOfView);
                }
                if self.near_clip() <= 0.0 {
                    return Err(WorldCameraError::ClipRange);
                }
            }
            WorldCameraProjection::Orthographic {
                horizontal,
                vertical,
            } => {
                if [horizontal, vertical]
                    .into_iter()
                    .any(|[minimum, maximum]| {
                        !minimum.is_finite() || !maximum.is_finite() || maximum <= minimum
                    })
                {
                    return Err(WorldCameraError::OrthographicBounds);
                }
            }
        }
        if !self.near_clip().is_finite()
            || !self.far_clip().is_finite()
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
