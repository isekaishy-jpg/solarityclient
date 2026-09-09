//! One stock perspective scene camera shared by all resident WMO roots.

use glam::{Mat4, Vec3};

use super::matrix;
use crate::collision::world_model_visibility::{
    WorldModelPortalProjectionFrame, WorldModelVisibilityError,
};

/// Native scene corners and projection, independent of the graphics API.
#[derive(Clone, Copy, Debug)]
pub struct WorldSceneCameraFrame {
    eye: Vec3,
    target: Vec3,
    relative_projection: Mat4,
    corners: [Vec3; 8],
    clip_planes: [[f32; 4]; 5],
}

impl WorldSceneCameraFrame {
    /// Builds 795400's ordinary perspective scene from the final world camera.
    ///
    /// The view is relative to the eye and uses 6BFE60's positive-forward
    /// convention. 6BF6D0 inverts view/projection separately, transforms its
    /// depth-scaled clip corners, then 795400 adds the world eye to each corner.
    /// 607DB8 supplies the original camera direction to view construction;
    /// the rounded world target remains separate for 7A6E00's local depth plane.
    ///
    /// # Errors
    /// Rejects nonfinite inputs, invalid perspective parameters or a degenerate
    /// camera basis that cannot produce finite portal clipping planes.
    pub fn perspective(
        eye: Vec3,
        target: Vec3,
        forward: Vec3,
        up: Vec3,
        vertical_fov_radians: f32,
        aspect_ratio: f32,
        clip_range: [f32; 2],
    ) -> Result<Self, WorldModelVisibilityError> {
        let [near, far] = clip_range;
        if !eye.is_finite() || !target.is_finite() || !forward.is_finite() || !up.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        if ![vertical_fov_radians, aspect_ratio, near, far]
            .into_iter()
            .all(f32::is_finite)
            || vertical_fov_radians <= 0.
            || f64::from(vertical_fov_radians) >= std::f64::consts::PI
            || aspect_ratio <= 0.
            || near <= 0.
            || far <= near
        {
            return Err(WorldModelVisibilityError::InvalidCameraProjection);
        }
        let view = matrix::view(forward, up);
        let projection = matrix::perspective(vertical_fov_radians, aspect_ratio, near, far);
        let inverse = matrix::multiply(
            matrix::inverse(projection).ok_or(WorldModelVisibilityError::DegenerateFrustum)?,
            matrix::inverse(view).ok_or(WorldModelVisibilityError::DegenerateFrustum)?,
        );
        // 6BF6D0 reconstructs these depths from the stored projection entries.
        let near = (-f64::from(projection[14]) / (f64::from(projection[10]) + 1.)) as f32;
        let far = (-f64::from(projection[14]) / (f64::from(projection[10]) - 1.)) as f32;
        let corners = std::array::from_fn(|index| {
            let depth = if index < 4 { near } else { far };
            let x = if index % 4 < 2 { -depth } else { depth };
            let y = if index % 4 == 0 || index % 4 == 3 {
                -depth
            } else {
                depth
            };
            let z = if index < 4 { -depth } else { depth };
            matrix::corner(inverse, [x, y, z, depth]) + eye
        });
        let relative_projection = Mat4::from_cols_array(&matrix::multiply(view, projection));
        let frame = WorldModelPortalProjectionFrame::from_frustum_corners(
            Mat4::IDENTITY,
            eye,
            eye,
            relative_projection,
            corners,
        )?;
        Ok(Self {
            eye,
            target,
            relative_projection,
            corners,
            clip_planes: frame.clip_planes,
        })
    }

    /// Returns native near/far corners for scene bounds and group frustum tests.
    #[must_use]
    pub const fn corners(&self) -> &[Vec3; 8] {
        &self.corners
    }

    /// Binds one root's retained forward/inverse placement to the shared camera.
    ///
    /// # Errors
    /// Rejects nonfinite root transforms or transformed camera coordinates.
    pub fn for_root(
        self,
        root_transform: Mat4,
        inverse_transform: Mat4,
    ) -> Result<WorldModelPortalProjectionFrame, WorldModelVisibilityError> {
        let local_camera = matrix::point(inverse_transform.to_cols_array(), self.eye);
        if !root_transform.is_finite()
            || !inverse_transform.is_finite()
            || !local_camera.is_finite()
        {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        Ok(WorldModelPortalProjectionFrame {
            root_transform,
            local_camera,
            world_camera: self.eye,
            relative_projection: self.relative_projection,
            clip_planes: self.clip_planes,
        })
    }

    /// Builds 7A6E00's forward plane after transforming both camera points.
    ///
    /// The native short-direction branch retains the unnormalized difference.
    /// Normalization and the plane offset use extended values before float
    /// stores; the stored local camera/target points still bound subtraction.
    ///
    /// # Errors
    /// Rejects nonfinite root transforms or transformed camera coordinates.
    pub fn local_forward_plane(
        self,
        inverse_transform: Mat4,
    ) -> Result<[f32; 4], WorldModelVisibilityError> {
        let eye = matrix::point(inverse_transform.to_cols_array(), self.eye);
        let target = matrix::point(inverse_transform.to_cols_array(), self.target);
        if !inverse_transform.is_finite() || !eye.is_finite() || !target.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        let eye = eye.as_dvec3();
        let mut direction = target.as_dvec3() - eye;
        let length_squared =
            (direction.y * direction.y + direction.z * direction.z) + direction.x * direction.x;
        if length_squared > f64::from(0.0001_f32) {
            direction *= 1. / length_squared.sqrt();
        }
        let distance = -((direction.z * eye.z + direction.y * eye.y) + direction.x * eye.x);
        Ok([
            direction.x as f32,
            direction.y as f32,
            direction.z as f32,
            distance as f32,
        ])
    }
}
