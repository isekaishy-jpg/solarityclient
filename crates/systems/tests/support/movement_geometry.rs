//! Controlled provider failure, independent of movement response decisions.

use glam::Vec3;
use solarity_systems::{MovementCollisionTriangle, MovementCollisionVolume, MovementGeometry};

pub struct Geometry<'a> {
    triangles: &'a [MovementCollisionTriangle],
    failure: u32,
    probes: u32,
}

impl<'a> Geometry<'a> {
    pub fn new(triangles: &'a [MovementCollisionTriangle], failure: u32) -> Self {
        Self {
            triangles,
            failure,
            probes: 0,
        }
    }
}

impl MovementGeometry for Geometry<'_> {
    type TriangleIdentity = usize;

    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        self.probes += 1;
        self.probes != self.failure
    }

    fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.triangles
    }

    fn triangle_identity(&self, triangle: usize) -> Option<usize> {
        (triangle < self.triangles.len()).then_some(triangle)
    }
}
