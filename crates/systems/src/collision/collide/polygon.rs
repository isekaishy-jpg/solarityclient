//! Bounded convex-polygon clipping used by native face sweeps.

use glam::Vec3;

use super::{CONTACT_TOLERANCE, MovementCollisionPlane};

/// A triangle gains at most one vertex per clipping plane. Fifteen slots match
/// `0x0075B710` and cover four extrusion planes plus eight other body planes.
pub(super) struct ContactPolygon {
    vertices: [Vec3; 15],
    len: usize,
}

impl ContactPolygon {
    /// Starts with the authored triangle's ordered vertices and no heap storage.
    pub(super) fn from_triangle(vertices: [Vec3; 3]) -> Self {
        let mut result = Self {
            vertices: [Vec3::ZERO; 15],
            len: 3,
        };
        result.vertices[..3].copy_from_slice(&vertices);
        result
    }

    pub(super) fn vertices(&self) -> &[Vec3] {
        &self.vertices[..self.len]
    }

    /// Clips to a plane's interior with the native whole-polygon tolerance and
    /// strict zero-crossing interpolation. Fewer than three vertices disappear.
    pub(super) fn clip(&mut self, plane: MovementCollisionPlane) {
        if self.len == 0 {
            return;
        }
        let mut distances = [0.0_f32; 15];
        let mut minimum = f64::from(f32::MAX);
        let mut maximum = -f64::from(f32::MAX);
        for (index, point) in self.vertices().iter().enumerate() {
            let distance = -plane.distance(*point);
            distances[index] = distance as f32;
            minimum = minimum.min(distance);
            maximum = maximum.max(distance);
        }
        if minimum > -f64::from(CONTACT_TOLERANCE) {
            return;
        }
        if maximum < f64::from(CONTACT_TOLERANCE) {
            self.len = 0;
            return;
        }

        let source = self.vertices;
        let source_len = self.len;
        self.len = 0;
        let mut previous = source_len - 1;
        for current in 0..source_len {
            let before = distances[previous];
            let after = distances[current];
            if (before < 0.0 && after > CONTACT_TOLERANCE)
                || (after < 0.0 && before > CONTACT_TOLERANCE)
            {
                let before = f64::from(before);
                let after = f64::from(after);
                let point = source[previous].as_dvec3()
                    - (source[current].as_dvec3() - source[previous].as_dvec3())
                        * (before / (after - before));
                self.push(point.as_vec3());
            }
            if after >= 0.0 {
                self.push(source[current]);
            }
            previous = current;
        }
        if self.len < 3 {
            self.len = 0;
        }
    }

    /// The initial triangle and at most twelve planes prove the fixed capacity.
    fn push(&mut self, point: Vec3) {
        self.vertices[self.len] = point;
        self.len += 1;
    }
}
