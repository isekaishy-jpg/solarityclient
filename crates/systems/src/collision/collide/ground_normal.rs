//! Body-box surface averaging from native `0x0075EE60` / `0x0075EC10`.

use glam::Vec3;

use super::polygon::ContactPolygon;
use super::{MovementCollisionPlane, MovementCollisionTriangle, MovementCollisionVolume};

const MINIMUM_UPWARD_NORMAL: f32 = f32::from_bits(0x3c8e_f859);

impl MovementCollisionVolume {
    /// Computes the presentation normal from the current ordered candidate set.
    ///
    /// Stock clips each sufficiently upward-facing triangle to the six-plane
    /// body box, averages the surviving authored normals without area weights,
    /// and normalizes once. No intersecting face yields the stock upright normal.
    /// The caller owns candidate collection and movement-mode admission; this
    /// query neither moves the unit nor modifies its collision continuation.
    #[must_use]
    pub fn ground_normal(&self, triangles: &[MovementCollisionTriangle]) -> Vec3 {
        let planes = [
            self.planes[1],
            self.planes[0],
            self.planes[2],
            self.planes[3],
            self.planes[4],
            MovementCollisionPlane::through(Vec3::NEG_Z, self.foot_origin()),
        ];
        let mut sum = Vec3::ZERO;
        let mut count = 0_u32;
        for triangle in triangles {
            if triangle.normal.z <= MINIMUM_UPWARD_NORMAL {
                continue;
            }
            let mut polygon = ContactPolygon::from_triangle(triangle.vertices);
            for plane in planes {
                polygon.clip(plane);
            }
            if !polygon.vertices().is_empty() {
                sum += triangle.normal;
                count += 1;
            }
        }
        if count == 0 {
            return Vec3::Z;
        }
        // 75ED43 retains both the reciprocal count and averaged components in
        // x87 registers until the final normalized XYZ stores.
        let average = sum.as_dvec3() / f64::from(count);
        (average / average.length()).as_vec3()
    }
}
