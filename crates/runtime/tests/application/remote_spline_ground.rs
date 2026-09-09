//! Walking splines consume scene admission and preserve native terrain corrections.

use glam::Vec3;
use solarity_network::{MonsterMove, MonsterMovePath, MovementSplineFacing};
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementGeometry, MovementGroundProfile,
    MovementIntervalRequest,
};

use super::RemoteUnit;
use crate::application::player_movement::{
    LocalMovementGeometry, RuntimePlayerMovementError, tests::owner,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Inclined resident floor plus an observable initial collection boundary.
struct Slope {
    triangles: [MovementCollisionTriangle; 2],
    ready: bool,
    collections: usize,
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
            ready: true,
            collections: 0,
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
        self.collections += 1;
        Ok(self.ready)
    }
}

/// Native packet preparation owns the initial forward flags and path clock.
fn walking_path(
    duration_ms: u32,
    end_z: f32,
    slope: &mut Slope,
) -> Result<RemoteUnit, Box<dyn std::error::Error>> {
    let (_, motion) = owner()?;
    let (transform, movement) = motion.snapshot();
    let mut remote = RemoteUnit::new(motion.identity, transform, movement, 0);
    remote.receive_path(
        &MonsterMove {
            guid: motion.identity.guid(),
            transport: None,
            control_byte: 0,
            start: [0.; 3],
            id: 17,
            facing_type: 0,
            facing: MovementSplineFacing::Direction,
            path: Some(MonsterMovePath {
                flags: 0,
                duration_ms,
                animation: None,
                parabolic: None,
                points: vec![[10., 0., end_z]],
            }),
        },
        0,
        1.,
        slope,
        |_| None,
    )?;
    Ok(remote)
}

#[test]
fn walking_spline_keeps_surface_height_and_normal_without_double_advancement() -> TestResult {
    let mut slope = Slope::new()?;
    let mut remote = walking_path(5000, 0., &mut slope)?;
    remote.scene_collision = true;
    for time in [250, 500, 750] {
        remote.advance(
            time,
            [0.5, 2., 1.],
            MovementGroundProfile::Other,
            &mut slope,
            |_| None,
        )?;
        let position = remote.snapshot().0.position();
        assert!(
            (position.x - time as f32 * 0.002).abs() < 0.02,
            "{position:?}"
        );
        assert!(
            (position.z - position.x * 0.25).abs() < 0.02,
            "{position:?}"
        );
        assert!((remote.ground_normal - Vec3::new(-0.25, 0., 1.).normalize()).length() < 0.00001);
        assert!(remote.snapshot().1.spline().is_some());
    }
    assert!(slope.collections >= 3);
    remote.advance(
        5000,
        [0.5, 2., 1.],
        MovementGroundProfile::Other,
        &mut slope,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0.position(), Vec3::new(10., 0., 0.));
    assert_ne!(
        remote.snapshot().1.spline().ok_or("completed path")?.flags & 0x400,
        0
    );
    Ok(())
}

#[test]
fn scene_rejected_spline_stores_raw_target_without_querying_terrain() -> TestResult {
    let mut slope = Slope::new()?;
    let mut remote = walking_path(5000, 0., &mut slope)?;
    remote.advance(
        250,
        [0.5, 2., 1.],
        MovementGroundProfile::Other,
        &mut slope,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0.position(), Vec3::new(0.5, 0., 0.));
    assert_eq!(slope.collections, 0);
    assert_eq!(remote.ground_normal, Vec3::Z);
    assert_eq!(
        remote.motion.as_ref().ok_or("path owner")?.elapsed_ms,
        500,
        "6EAC40 and the scene-rejected 6E9E20 branch each add the interval"
    );
    Ok(())
}

#[test]
fn excessive_spline_displacement_snaps_before_scene_collision() -> TestResult {
    let mut slope = Slope::new()?;
    let mut remote = walking_path(500, 0., &mut slope)?;
    remote.scene_collision = true;
    remote.advance(
        250,
        [0.5, 2., 1.],
        MovementGroundProfile::Other,
        &mut slope,
        |_| None,
    )?;
    assert_eq!(remote.snapshot().0.position(), Vec3::new(5., 0., 0.));
    assert_eq!(slope.collections, 0);
    assert_eq!(
        remote.motion.as_ref().ok_or("path owner")?.elapsed_ms,
        250,
        "6E9C30 snaps after the outer clock increment"
    );
    Ok(())
}

#[test]
fn unavailable_spline_geometry_uses_travel_normal_and_sampled_target() -> TestResult {
    let mut slope = Slope::new()?;
    slope.ready = false;
    let mut remote = walking_path(5000, 2., &mut slope)?;
    remote.scene_collision = true;
    remote.advance(
        250,
        [0.5, 2., 1.],
        MovementGroundProfile::Other,
        &mut slope,
        |_| None,
    )?;
    assert!((remote.snapshot().0.position() - Vec3::new(0.5, 0., 0.1)).length() < 0.00001);
    assert!((remote.ground_normal - Vec3::new(-0.2, 0., 1.).normalize()).length() < 0.00001);
    assert_eq!(slope.collections, 1);
    Ok(())
}
