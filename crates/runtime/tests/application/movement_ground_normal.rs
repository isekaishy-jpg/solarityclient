//! The runtime must refresh normals at the final collision position, including landing.

use std::collections::VecDeque;

use glam::Vec3;
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementGeometry, MovementIntervalRequest,
};

use super::super::{LocalMovementGeometry, RuntimePlayerMovementError, tests::owner};

/// One authored inclined floor with independently known slope and normal.
struct Slope {
    triangles: [MovementCollisionTriangle; 2],
}

impl Slope {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let a = Vec3::new(-100., -100., -25.);
        let b = Vec3::new(100., -100., 25.);
        let c = Vec3::new(100., 100., 25.);
        let d = Vec3::new(-100., 100., -25.);
        Ok(Self {
            triangles: [
                MovementCollisionTriangle::new([a, b, c])?,
                MovementCollisionTriangle::new([a, c, d])?,
            ],
        })
    }
}

impl MovementGeometry for Slope {
    type TriangleIdentity = usize;
    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        true
    }
    fn triangles(&self) -> &[MovementCollisionTriangle] {
        &self.triangles
    }
    fn triangle_identity(&self, index: usize) -> Option<usize> {
        self.triangles.get(index).map(|_| index)
    }
}

impl LocalMovementGeometry for Slope {
    fn collect(&mut self, _: MovementIntervalRequest) -> Result<bool, RuntimePlayerMovementError> {
        Ok(true)
    }
}

#[test]
fn walking_interval_publishes_the_inclined_surface_normal() -> Result<(), Box<dyn std::error::Error>>
{
    let (_, mut movement) = owner()?;
    movement.flags = 1;
    movement.reanchor()?;
    movement.interval(100, [0.5, 2., 1.], &mut Slope::new()?, &mut VecDeque::new())?;
    assert!(movement.position.x > 0.);
    assert!((movement.position.z - movement.position.x * 0.25).abs() < 0.02);
    assert!(
        (movement.world_ground_normal() - Vec3::new(-0.25, 0., 1.).normalize()).length() < 0.00001
    );
    Ok(())
}

#[test]
fn landing_exit_publishes_the_surface_normal_before_returning()
-> Result<(), Box<dyn std::error::Error>> {
    let (_, mut movement) = owner()?;
    movement.acquire_mover(&mut VecDeque::new())?;
    movement.interval(100, [0.5, 2., 1.], &mut Slope::new()?, &mut VecDeque::new())?;
    assert_eq!(movement.flags & 0x1000, 0, "initial support must land");
    assert!(
        (movement.world_ground_normal() - Vec3::new(-0.25, 0., 1.).normalize()).length() < 0.00001
    );
    Ok(())
}
