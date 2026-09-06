//! Contact identities must survive private queries replacing candidate arrays.

use glam::{Vec2, Vec3};
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementFallAdmission,
    MovementFallAdvancePolicy, MovementFallContinuation, MovementFallInterval, MovementFallMode,
    MovementFallPhase, MovementFallSnapshot, MovementFallState, MovementGeometry,
    MovementGroundContinuation, MovementGroundInterval, MovementGroundProfile,
    MovementGroundSnapshot, MovementGroundState, MovementSupportProfile,
};
use std::error::Error;

enum Change {
    Reorder,
    Truncate,
    Fail,
}

struct Geometry {
    triangles: Vec<MovementCollisionTriangle>,
    identities: Vec<&'static str>,
    probes: usize,
    change: Change,
}

impl MovementGeometry for Geometry {
    type TriangleIdentity = &'static str;

    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        self.probes += 1;
        if self.probes == 2 {
            match self.change {
                Change::Reorder => {
                    self.triangles.reverse();
                    self.identities.reverse();
                }
                Change::Truncate => {
                    self.triangles.truncate(1);
                    self.identities.truncate(1);
                }
                Change::Fail => {
                    self.triangles.clear();
                    self.identities.clear();
                    return false;
                }
            }
        }
        true
    }

    fn triangles(&self) -> &[MovementCollisionTriangle] {
        &self.triangles
    }

    fn triangle_identity(&self, index: usize) -> Option<&'static str> {
        self.identities.get(index).copied()
    }
}

fn wall(x: f32) -> Result<MovementCollisionTriangle, Box<dyn Error>> {
    Ok(MovementCollisionTriangle::new([
        Vec3::new(x, -10., -9.),
        Vec3::new(x, -10., 11.),
        Vec3::new(x, 10., 1.),
    ])?)
}

#[test]
fn ground_copies_identity_before_step_and_keeps_native_late_index_guard()
-> Result<(), Box<dyn Error>> {
    let state = MovementGroundState::new(MovementGroundSnapshot {
        position: Vec3::ZERO,
        step_anchor: None,
        fall_time_ms: 73,
        launch_height: 101.,
        initial_downward_speed: -7.95,
        horizontal_direction: Vec2::X,
        horizontal_speed: 7.,
        direction: Vec3::X,
        mode: MovementFallMode::Normal,
        fall_admission: MovementFallAdmission::Allowed,
    })?;
    let interval = MovementGroundInterval {
        duration_ms: 1000,
        distance: 5.,
        direction: Vec2::X,
        radius: 0.5,
        height: 2.,
        profile: MovementGroundProfile::PlayerControlled { step_height: 1. },
    };
    let mut geometry = Geometry {
        triangles: vec![wall(3.)?, wall(3.)?],
        identities: vec!["first", "second"],
        probes: 0,
        change: Change::Reorder,
    };
    let result = state.advance_with_geometry(interval, &mut geometry)?;
    assert!(geometry.probes > 2);
    assert_eq!(result.contact_triangle, Some("second"));
    assert!(matches!(
        result.continuation,
        MovementGroundContinuation::Grounded(_)
    ));
    assert!(!result.geometry_unavailable);
    assert_eq!(geometry.identities, ["second", "first"]);

    geometry.triangles = vec![wall(3.)?, wall(3.)?];
    geometry.identities = vec!["first", "second"];
    geometry.probes = 0;
    geometry.change = Change::Truncate;
    let result = state.advance_with_geometry(interval, &mut geometry)?;
    assert!(geometry.probes > 2);
    assert!(matches!(
        result.continuation,
        MovementGroundContinuation::Falling(_)
    ));
    assert!(result.reset_motion_anchor);
    assert!(result.contact_triangle.is_none());
    assert!(!result.geometry_unavailable);
    Ok(())
}

#[test]
fn failed_fall_refresh_retains_prior_contact_after_candidates_are_cleared()
-> Result<(), Box<dyn Error>> {
    let state = MovementFallState::new(MovementFallSnapshot {
        position: Vec3::ZERO,
        fall_time_ms: 73,
        launch_height: 0.,
        initial_downward_speed: 0.,
        horizontal_direction: Vec2::X,
        horizontal_speed: 7.,
        direction: Vec3::X,
        mode: MovementFallMode::Normal,
        phase: MovementFallPhase::Falling,
    })?;
    let mut geometry = Geometry {
        triangles: vec![wall(2.)?],
        identities: vec!["wall"],
        probes: 0,
        change: Change::Fail,
    };
    let result = state.advance_with_geometry(
        MovementFallInterval {
            duration_ms: 1000,
            displacement: Vec3::new(5., 0., -2.),
            radius: 0.5,
            height: 2.,
            support_profile: MovementSupportProfile::PlayerControlled,
            policy: MovementFallAdvancePolicy::Live,
        },
        &mut geometry,
    )?;
    assert_eq!(geometry.probes, 2);
    assert!(geometry.triangles.is_empty());
    assert_eq!(result.contact_triangle, Some("wall"));
    assert!(result.geometry_unavailable);
    assert_eq!(result.consumed_ms, 1000);
    assert_eq!(result.skipped_time_ms, 824);
    let MovementFallContinuation::Airborne(state) = result.continuation else {
        return Err("unexpected landing".into());
    };
    assert_eq!(state.snapshot().fall_time_ms, 249);
    assert!((state.snapshot().position - Vec3::new(0.88204473, 0., -0.6)).length() < 0.0001);
    Ok(())
}
