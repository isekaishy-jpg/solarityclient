//! Ground/fall response consumes the actual resident geometry provider.

use super::*;
use glam::Vec2;
use solarity_runtime::{RuntimeMovementGeometry, RuntimeMovementGeometryFailure};
use solarity_systems::{
    MovementCollisionVolume, MovementFallAdmission, MovementFallAdvancePolicy,
    MovementFallContinuation, MovementFallInterval, MovementFallMode, MovementFallPhase,
    MovementFallSnapshot, MovementFallState, MovementGeometry, MovementGroundContinuation,
    MovementGroundInterval, MovementGroundProfile, MovementGroundSnapshot, MovementGroundState,
    MovementIntervalMode, MovementIntervalRequest, MovementSupportProfile,
};

fn falling(position: Vec3) -> Result<MovementFallState, Box<dyn Error>> {
    Ok(MovementFallState::new(MovementFallSnapshot {
        position,
        fall_time_ms: 73,
        launch_height: position.z,
        initial_downward_speed: 0.,
        horizontal_direction: Vec2::ZERO,
        horizontal_speed: 0.,
        direction: Vec3::ZERO,
        mode: MovementFallMode::Normal,
        phase: MovementFallPhase::Falling,
    })?)
}

#[test]
fn response_uses_refreshed_resident_geometry_and_preserves_pending_cause()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new(false)?;
    scene.synchronize()?;
    let position = Vec3::new(1000., 5800., 11.);
    let request = MovementIntervalRequest {
        position,
        radius: 0.5,
        height: 2.,
        distance: 0.,
        direction: Vec3::X,
        duration_ms: 16,
        mode: MovementIntervalMode::SwimmingOrFlying,
    };
    let mut output = RuntimeMovementQuery::new();
    let mut geometry = RuntimeMovementGeometry::new(
        &mut scene.terrain,
        &scene.world,
        &scene.objects,
        0x100111,
        MovementBspCacheMode::Enabled,
        &mut output,
    );
    // Seed a complete resident region above the floor, forcing the response's
    // first sweep to refill its candidate array through the provider boundary.
    assert_eq!(
        geometry.collect_interval(request)?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(geometry.triangles().is_empty());
    let interval = MovementFallInterval {
        duration_ms: 1000,
        displacement: Vec3::NEG_Z * 2.,
        radius: 0.5,
        height: 2.,
        support_profile: MovementSupportProfile::PlayerControlled,
        policy: MovementFallAdvancePolicy::Live,
    };
    let result = falling(position)?.advance_with_geometry(interval, &mut geometry)?;
    let MovementFallContinuation::Landed {
        position: landed, ..
    } = result.continuation
    else {
        return Err("refreshed resident floor did not stop falling".into());
    };
    assert!((landed.z - 10.).abs() < 0.01);
    assert!(result.consumed_ms < interval.duration_ms);
    assert_eq!(result.skipped_time_ms, 0);
    assert!(!result.geometry_unavailable);
    assert!(matches!(
        result.contact_triangle,
        Some(RuntimeMovementOwner::Static(
            RuntimeStaticMovementOwner::Terrain { .. }
        ))
    ));
    assert!(geometry.failure().is_none());

    let profile = MovementGroundProfile::PlayerControlled { step_height: 1. };
    assert_eq!(
        geometry.collect_interval(MovementIntervalRequest {
            position: landed,
            distance: 5.,
            duration_ms: 714,
            mode: MovementIntervalMode::Grounded(profile),
            ..request
        })?,
        RuntimeStaticMovementResidency::Ready
    );
    let ground = MovementGroundState::new(MovementGroundSnapshot {
        position: landed,
        step_anchor: None,
        fall_time_ms: 0,
        launch_height: landed.z,
        initial_downward_speed: 0.,
        horizontal_direction: Vec2::X,
        horizontal_speed: 7.,
        direction: Vec3::X,
        mode: MovementFallMode::Normal,
        fall_admission: MovementFallAdmission::Allowed,
    })?;
    let result = ground.advance_with_geometry(
        MovementGroundInterval {
            duration_ms: 714,
            distance: 5.,
            direction: Vec2::X,
            radius: 0.5,
            height: 2.,
            profile,
        },
        &mut geometry,
    )?;
    let MovementGroundContinuation::Grounded(next) = result.continuation else {
        return Err("flat resident floor did not retain grounded movement".into());
    };
    assert!((next.snapshot().position - Vec3::new(1005., 5800., 10.)).length() < 0.01);
    assert!(matches!(
        result.contact_triangle,
        Some(RuntimeMovementOwner::Static(
            RuntimeStaticMovementOwner::Terrain { .. }
        ))
    ));
    assert!(!result.geometry_unavailable);

    // The seed region is complete, but this fall sweep reaches an unavailable
    // declared neighbor. Native consumed time and retained fall time differ.
    let edge = Vec3::new(1000., 5335., 11.);
    assert_eq!(
        geometry.collect_interval(MovementIntervalRequest {
            position: edge,
            ..request
        })?,
        RuntimeStaticMovementResidency::Ready
    );
    let result = falling(edge)?.advance_with_geometry(
        MovementFallInterval {
            displacement: Vec3::new(0., -2., -1.),
            ..interval
        },
        &mut geometry,
    )?;
    assert!(result.geometry_unavailable);
    assert_eq!(result.consumed_ms, 1000);
    assert_eq!(result.skipped_time_ms, 1000);
    let MovementFallContinuation::Airborne(next) = result.continuation else {
        return Err("unavailable geometry became a landing".into());
    };
    assert_eq!(next.snapshot().position, edge);
    assert_eq!(next.snapshot().fall_time_ms, 73);
    assert!(geometry.triangles().is_empty());
    assert!(geometry.cached_bounds().is_none());
    assert!(
        matches!(geometry.failure(), Some(RuntimeMovementGeometryFailure::Pending(RuntimeStaticMovementResidency::PendingTile { tile })) if *tile == TerrainTileIndex::new(22, 30).ok_or("neighbor tile")?)
    );
    assert!(!MovementGeometry::prepare_sweep(
        &mut geometry,
        &MovementCollisionVolume::new(edge, 0.5, 2.)?,
        Vec3::X,
        1.
    ));
    assert!(matches!(
        geometry.failure(),
        Some(RuntimeMovementGeometryFailure::Pending(_))
    ));
    geometry.collect_interval(request)?;
    assert!(geometry.failure().is_none());
    Ok(())
}
