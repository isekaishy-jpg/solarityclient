//! Native water-only movement collection (`0x20000`) before plane inversion.

mod terrain;
mod world_model;

pub use terrain::append_terrain_liquid_movement;

use glam::Vec3;

use super::MovementCollisionBounds;

/// 7CE960/7C7790 test only Z after the native grid rectangle selects X/Y.
fn admits_height(bounds: MovementCollisionBounds, vertices: [Vec3; 3]) -> bool {
    let tolerance = f64::from(0.019_444_443_f32);
    !vertices
        .iter()
        .all(|point| f64::from(point.z) - f64::from(bounds.minimum().z) + tolerance < 0.0)
        && !vertices
            .iter()
            .all(|point| f64::from(bounds.maximum().z) - f64::from(point.z) + tolerance < 0.0)
}
