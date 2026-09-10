//! Stock world bounds frusta, separate from fixed camera portal clipping.

use glam::{Mat4, Vec3};

use super::super::frame::frustum_planes;
use super::{bounds, matrix};
use crate::collision::{MovementCollisionBounds, WorldModelVisibilityError};

/// Eight world corners and six inward planes for native scene bounds tests.
#[derive(Clone, Copy, Debug)]
pub struct WorldSceneFrustum {
    corners: [Vec3; 8],
    clip_planes: [[f32; 4]; 6],
}

impl WorldSceneFrustum {
    /// Transforms a stored scene clip into a WMO root's local coordinates.
    ///
    /// 78FB00/983F40 transform all eight corners through 4C2300, then rebuild
    /// the six planes. The affine point operation retains extended products
    /// until each component store; transforming planes or using an f32 matrix
    /// multiply changes native batch-boundary decisions. No W divide occurs.
    ///
    /// # Errors
    /// Rejects non-finite transforms and degenerate or non-finite result planes.
    pub fn transformed(self, transform: Mat4) -> Result<Self, WorldModelVisibilityError> {
        if !transform.is_finite() {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        let transform = transform.to_cols_array();
        Self::from_corners(self.corners.map(|corner| matrix::point(transform, corner)))
    }

    /// Constructs 984240's scene planes, including the mirrored near face.
    pub(super) fn from_corners(corners: [Vec3; 8]) -> Result<Self, WorldModelVisibilityError> {
        let faces = frustum_planes(corners)?;
        let clip_planes = std::array::from_fn(|index| {
            if index < 5 {
                faces[index]
            } else {
                bounds::near_plane(faces[4], corners[2])
            }
        });
        if !clip_planes.into_iter().flatten().all(f32::is_finite) {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        Ok(Self {
            corners,
            clip_planes,
        })
    }

    /// Crops the full camera corners with 790AF0/790E20's float store schedule.
    pub(super) fn for_window(self, window: [f32; 4]) -> Result<Self, WorldModelVisibilityError> {
        if !window.into_iter().all(f32::is_finite) {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        if window[0] >= window[2] || window[1] >= window[3] {
            return Err(WorldModelVisibilityError::DegenerateFrustum);
        }
        let [n0, n1, n2, n3, f0, f1, f2, f3] = self.corners;
        let near = crop_face([n0, n1, n2, n3], window);
        let far = crop_face([f0, f1, f2, f3], window);
        Self::from_corners([
            near[0], near[1], near[2], near[3], far[0], far[1], far[2], far[3],
        ])
    }

    /// Returns near then far corners, each bottom-left through bottom-right.
    #[must_use]
    pub const fn corners(&self) -> &[Vec3; 8] {
        &self.corners
    }

    /// Returns the inward top, bottom, right, left, far and near world planes.
    #[must_use]
    pub const fn clip_planes(&self) -> &[[f32; 4]; 6] {
        &self.clip_planes
    }

    /// Applies 9839E0's supporting-corner test and native negative tolerance.
    #[must_use]
    pub fn intersects_bounds(self, bounds: MovementCollisionBounds) -> bool {
        bounds::intersects(&self.clip_planes, bounds)
    }

    /// Applies 983FB0/983D20's sphere test without the AABB tolerance.
    /// Products remain extended until comparison, in native X, Z, Y order.
    /// Invalid spheres are rejected.
    #[must_use]
    pub fn intersects_sphere(self, center: Vec3, radius: f32) -> bool {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        let center = center.as_dvec3();
        self.clip_planes.into_iter().all(|plane| {
            let [x, y, z, offset] = plane.map(f64::from);
            ((x * center.x + z * center.z) + y * center.y) + offset >= -f64::from(radius)
        })
    }
}

/// Interpolates one native near/far face. The asymmetric spills below come from
/// both original window routines; a generic bilinear lerp changes stored bits.
fn crop_face(points: [Vec3; 4], window: [f32; 4]) -> [Vec3; 4] {
    let p = points.map(Vec3::as_dvec3);
    let [min_y, min_x, max_y, max_x] = window.map(f64::from);
    let top_delta = p[2] - p[1];
    let mut top_low_product = top_delta * min_x;
    top_low_product.y = f64::from(top_low_product.y as f32);
    let top_low = (p[1] + top_low_product).as_vec3().as_dvec3();
    let top_high = (p[1] + top_delta * max_x).as_vec3().as_dvec3();
    let bottom_low = p[0] + (p[3] - p[0]) * min_x;
    let bottom_high = p[0] + (p[3] - p[0]) * max_x;
    let mut low = top_low - bottom_low;
    low.y = f64::from(low.y as f32);
    low.z = f64::from(low.z as f32);
    let mut first = low * min_y;
    first.x = f64::from(first.x as f32);
    first.y = f64::from(first.y as f32);
    low.x = f64::from(low.x as f32);
    let mut second = low * max_y;
    second.x = f64::from(second.x as f32);
    second.y = f64::from(second.y as f32);
    let high = top_high - bottom_high;
    let mut fourth = high * min_y;
    fourth.x = f64::from(fourth.x as f32);
    fourth.y = f64::from(fourth.y as f32);
    [
        (first + bottom_low).as_vec3(),
        (second + bottom_low).as_vec3(),
        (high * max_y + bottom_high).as_vec3(),
        (fourth + bottom_high).as_vec3(),
    ]
}
