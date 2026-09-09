//! Camera transforms and native five-plane portal clipping frame.

use crate::collision::world_model_visibility::WorldModelVisibilityError;
use glam::{Mat4, Vec3};

/// Camera and placement inputs retained by native WMO portal projection.
#[derive(Clone, Copy, Debug)]
pub struct WorldModelPortalProjectionFrame {
    /// Root-local to world transform.
    pub root_transform: Mat4,
    /// Camera position in root-local coordinates, used by the near-portal test.
    pub local_camera: Vec3,
    /// World camera position subtracted before projection.
    pub world_camera: Vec3,
    /// View/projection matrix operating on world positions relative to the eye.
    pub relative_projection: Mat4,
    /// Inward world-space planes ordered top, bottom, right, left, far.
    /// The near plane is deliberately excluded from portal clipping.
    pub clip_planes: [[f32; 4]; 5],
}

impl WorldModelPortalProjectionFrame {
    /// Builds the five stock portal planes from world-space frustum corners.
    ///
    /// Both faces use bottom-left, top-left, top-right, bottom-right order in
    /// clip space, with the near face first. The corners must come from the
    /// stock positive-forward view convention, whose basis is mirrored relative
    /// to the usual left-handed view. Corner coordinates already include the eye.
    /// The supplied projection operates on positions relative to that eye.
    ///
    /// # Errors
    /// Rejects nonfinite inputs or a frustum with degenerate clipping faces.
    pub fn from_frustum_corners(
        root_transform: Mat4,
        local_camera: Vec3,
        world_camera: Vec3,
        relative_projection: Mat4,
        corners: [Vec3; 8],
    ) -> Result<Self, WorldModelVisibilityError> {
        let clip_planes = frustum_planes(corners)?;
        let frame = Self {
            root_transform,
            local_camera,
            world_camera,
            relative_projection,
            clip_planes,
        };
        if !frame.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        Ok(frame)
    }

    pub(super) fn is_finite(self) -> bool {
        self.root_transform.is_finite()
            && self.relative_projection.is_finite()
            && self.local_camera.is_finite()
            && self.world_camera.is_finite()
            && self.clip_planes.into_iter().flatten().all(f32::is_finite)
    }
}

/// Builds 983E70's five shared portal/scene faces from stored world corners.
pub(super) fn frustum_planes(
    corners: [Vec3; 8],
) -> Result<[[f32; 4]; 5], WorldModelVisibilityError> {
    if corners.iter().any(|corner| !corner.is_finite()) {
        return Err(WorldModelVisibilityError::NonFiniteCoordinates);
    }
    let mut clip_planes = [[0.; 4]; 5];
    // 983E70 passes these corner triplets to 7912C0 in this order.
    for (output, [a, b, c]) in
        clip_planes
            .iter_mut()
            .zip([[1, 5, 6], [0, 7, 4], [0, 4, 5], [3, 6, 7], [5, 4, 6]])
    {
        let origin = corners[a].as_dvec3();
        // Differences remain extended, but the cross product spills to
        // floats before normalization. D uses the unspilled unit normal.
        let left = corners[b].as_dvec3() - origin;
        let right = corners[c].as_dvec3() - origin;
        let cross = Vec3::new(
            product_difference(left.y, right.z, left.z, right.y) as f32,
            product_difference(left.z, right.x, left.x, right.z) as f32,
            product_difference(left.x, right.y, left.y, right.x) as f32,
        )
        .as_dvec3();
        let length_squared = (cross.x * cross.x + cross.y * cross.y) + cross.z * cross.z;
        if !length_squared.is_finite() || length_squared == 0. {
            return Err(WorldModelVisibilityError::DegenerateFrustum);
        }
        let inverse_length = 1. / length_squared.sqrt();
        let normal = cross * inverse_length;
        // Factor the common reciprocal out of D's dot product to avoid
        // losing the x87 cancellation precision when the eye is near zero.
        let distance =
            -((cross.y * origin.y + origin.x * cross.x) + origin.z * cross.z) * inverse_length;
        *output = [
            normal.x as f32,
            normal.y as f32,
            normal.z as f32,
            distance as f32,
        ];
    }
    Ok(clip_planes)
}

/// Retains the cancellation precision of 7912C0's extended cross products.
/// Compensated products avoid rounding each multiplication to f64 before the
/// subtraction. Exact cancellation preserves the native arithmetic zero sign.
fn product_difference(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let product = c * d;
    let difference = a.mul_add(b, -product) + (-c).mul_add(d, product);
    if difference == 0. {
        a * b - product
    } else {
        difference
    }
}
