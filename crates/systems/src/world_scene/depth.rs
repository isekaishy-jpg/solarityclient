//! The 64 outdoor depth lists populated by `792E60`/`792AD0` before traversal.

use glam::Vec3;
use thiserror::Error;

/// Invalid camera or model bounds at the native outdoor scene boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum WorldSceneDepthError {
    /// The camera must have finite, distinct eye and target positions.
    #[error("scene depth camera is nonfinite or degenerate")]
    InvalidCamera,
    /// Bounds must be finite and ordered along every axis.
    #[error("scene depth model bounds are nonfinite or unordered")]
    InvalidBounds,
}

/// Camera-relative outdoor depth admission, independent of draw visibility.
///
/// `79A790` visits all 64 depth lists. Their M2 registrations receive the
/// movement-collision callback bit even when later frustum tests reject a draw.
/// A caller must separately establish exterior traversal and WMO membership.
#[derive(Clone, Copy, Debug)]
pub struct WorldSceneDepthFrame {
    eye: Vec3,
    target: Vec3,
    plane: [f32; 4],
    group_plane: [f32; 4],
}

impl WorldSceneDepthFrame {
    /// Builds `795400`'s horizontal depth plane with its float-store boundaries.
    ///
    /// # Errors
    /// Rejects nonfinite or coincident camera positions.
    pub fn new(eye: Vec3, target: Vec3) -> Result<Self, WorldSceneDepthError> {
        if !eye.is_finite() || !target.is_finite() || eye == target {
            return Err(WorldSceneDepthError::InvalidCamera);
        }
        let delta = target.as_dvec3() - eye.as_dvec3();
        let inverse = 1.0 / ((delta.y * delta.y + delta.z * delta.z) + delta.x * delta.x).sqrt();
        // X stays in x87; Y reloads its rounded difference before scaling.
        let mut x = delta.x * inverse;
        let mut y = f64::from(delta.y as f32) * inverse;
        let z = f64::from(delta.z as f32) * inverse;
        // 799310 uses the full view plane at CD8F80. Outdoor insertion
        // separately uses the horizontal plane at CD8F90.
        let group_offset = -((x * f64::from(eye.x) + y * f64::from(eye.y)) + z * f64::from(eye.z));
        let group_plane = [x as f32, y as f32, z as f32, group_offset as f32];
        let square = x * x + y * y;
        if square > f64::from(f32::from_bits(0x38d1_b717)) {
            let horizontal_inverse = 1.0 / square.sqrt();
            x *= horizontal_inverse;
            y *= horizontal_inverse;
        }
        let offset = -(y * f64::from(eye.y) + x * f64::from(eye.x));
        Ok(Self {
            eye,
            target,
            plane: [x as f32, y as f32, 0., offset as f32],
            group_plane,
        })
    }

    /// Selects `790650`'s leading AABB corner and the shared insertion bucket.
    ///
    /// Bounds use the raw registered transform, before unit ground-normal tilt.
    /// `None` leaves the registration unvisited by the outdoor depth traversal.
    /// Units behind the eye enter bucket zero, irrespective of draw visibility.
    /// `792AD0` uses the same bucket for static WMO groups masked by MOGI
    /// 0x10008. Their bounds come from the placed MOGI box, and their callbacks
    /// additionally require the outdoor window and portal traversal.
    ///
    /// # Errors
    /// Rejects nonfinite, reversed, or unrepresentable model bounds.
    pub fn depth_bin(self, bounds: [Vec3; 2]) -> Result<Option<u8>, WorldSceneDepthError> {
        let depth = self.extended_leading_depth(bounds, self.plane)?;
        if depth <= 0.0 {
            return Ok(Some(0));
        }
        let scaled = (depth * f64::from(f32::from_bits(0x3cf5_c28f))) as f32;
        if !scaled.is_finite() {
            return Err(WorldSceneDepthError::InvalidBounds);
        }
        let bucket = (f64::from(scaled) - 0.5).round_ties_even();
        Ok((bucket < 64.0).then_some(bucket as u8))
    }

    /// Returns 799310's stored group depth for the 78FB60 model size gate.
    ///
    /// # Errors
    /// Rejects nonfinite, reversed, or unrepresentable bounds.
    pub fn leading_depth(self, bounds: [Vec3; 2]) -> Result<f32, WorldSceneDepthError> {
        let depth = self.extended_leading_depth(bounds, self.group_plane)? as f32;
        if !depth.is_finite() {
            return Err(WorldSceneDepthError::InvalidBounds);
        }
        Ok(depth)
    }

    /// 7998A0 queues an exterior group's doodad by horizontal sphere depth,
    /// clamped to the group's currently visited bucket.
    ///
    /// # Errors
    /// Rejects invalid spheres and bucket indices outside the 64 scene lists.
    pub fn doodad_depth_bin(
        self,
        center: Vec3,
        radius: f32,
        minimum_bin: u8,
    ) -> Result<Option<u8>, WorldSceneDepthError> {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 || minimum_bin >= 64 {
            return Err(WorldSceneDepthError::InvalidBounds);
        }
        let [x, y, z, offset] = self.plane.map(f64::from);
        let center = center.as_dvec3();
        let depth = ((y * center.y + z * center.z) + x * center.x) + offset - f64::from(radius);
        if depth <= 0.0 {
            return Ok(Some(minimum_bin));
        }
        let scaled = (depth * f64::from(f32::from_bits(0x3cf5_c28f))) as f32;
        if !scaled.is_finite() {
            return Err(WorldSceneDepthError::InvalidBounds);
        }
        let bin = (f64::from(scaled) - 0.5).round_ties_even();
        Ok((bin < 64.0).then(|| (bin as u8).max(minimum_bin)))
    }

    fn extended_leading_depth(
        self,
        bounds: [Vec3; 2],
        plane: [f32; 4],
    ) -> Result<f64, WorldSceneDepthError> {
        let [minimum, maximum] = bounds;
        if !minimum.is_finite() || !maximum.is_finite() || minimum.cmpgt(maximum).any() {
            return Err(WorldSceneDepthError::InvalidBounds);
        }
        let corner = Vec3::select(self.eye.cmple(self.target), minimum, maximum);
        let [x, y, z, offset] = plane.map(f64::from);
        let depth = ((z * f64::from(corner.z) + y * f64::from(corner.y)) + x * f64::from(corner.x))
            + offset;
        Ok(depth)
    }
}
