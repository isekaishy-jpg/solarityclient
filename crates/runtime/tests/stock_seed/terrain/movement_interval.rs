//! Expanded runtime collection supplies actual terrain to the fall interval.

use super::*;
use glam::Vec2;
use solarity_runtime::RuntimeMovementGeometry;
use solarity_systems::{
    MovementCollisionVolume, MovementFallAdvancePolicy, MovementFallContinuation,
    MovementFallInterval, MovementFallMode, MovementFallPhase, MovementFallSnapshot,
    MovementFallState, MovementFallTrajectory, MovementGroundProfile, MovementIntervalMode,
    MovementIntervalRequest, MovementSupportProfile,
};

#[test]
fn sweep_cache_refreshes_union_and_invalidates_pending_or_failed_geometry()
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
    let volume = MovementCollisionVolume::new(position, request.radius, request.height)?;
    let mut query = RuntimeMovementQuery::new();
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x100111,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        assert!(geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.).is_err());
        assert_eq!(
            geometry.collect_interval(request)?,
            RuntimeStaticMovementResidency::Ready
        );
        let initial = geometry.cached_bounds().ok_or("missing initial coverage")?;
        assert!(geometry.triangles().is_empty());

        // The initial region has no floor. The downward probe must recollect
        // the old region joined to the expanded endpoint before narrow phase.
        assert_eq!(
            geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.)?,
            RuntimeStaticMovementResidency::Ready
        );
        let expanded = geometry
            .cached_bounds()
            .ok_or("missing refreshed coverage")?;
        assert_eq!(
            Some(expanded),
            volume.sweep_refresh_bounds(Vec3::NEG_Z, 2., initial)?
        );
        assert!(expanded.minimum().cmple(initial.minimum()).all());
        assert!(expanded.maximum().cmpge(initial.maximum()).all());
        let hit = volume.sweep(Vec3::NEG_Z * 2., geometry.triangles())?;
        let owner = geometry
            .owner(hit.last_triangle().ok_or("missing floor contact")?)
            .ok_or("missing floor owner")?;
        assert!(matches!(
            owner,
            RuntimeMovementOwner::Static(RuntimeStaticMovementOwner::Terrain { .. })
        ));
        assert!((hit.distance() - 1.).abs() < 0.01);
        let candidates = geometry.triangles().as_ptr();
        assert_eq!(
            geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.)?,
            RuntimeStaticMovementResidency::Ready
        );
        assert_eq!(geometry.cached_bounds(), Some(expanded));
        assert_eq!(geometry.triangles().as_ptr(), candidates);

        let moved =
            MovementCollisionVolume::new(position + Vec3::X * 3., request.radius, request.height)?;
        assert_eq!(
            geometry.prepare_sweep(&moved, Vec3::X, 2.)?,
            RuntimeStaticMovementResidency::Ready
        );
        let union = geometry.cached_bounds().ok_or("missing union coverage")?;
        assert_eq!(
            Some(union),
            moved.sweep_refresh_bounds(Vec3::X, 2., expanded)?
        );
        assert!(union.minimum().cmple(expanded.minimum()).all());
        assert!(union.maximum().cmpge(expanded.maximum()).all());
        assert!(!geometry.triangles().is_empty());

        // Only the refresh enters the declared, unavailable neighbor tile.
        let edge = MovementIntervalRequest {
            position: Vec3::new(1000., 5335., 11.),
            ..request
        };
        assert_eq!(
            geometry.collect_interval(edge)?,
            RuntimeStaticMovementResidency::Ready
        );
        let edge_volume = MovementCollisionVolume::new(edge.position, edge.radius, edge.height)?;
        assert_eq!(
            geometry.prepare_sweep(&edge_volume, Vec3::NEG_Y, 2.)?,
            RuntimeStaticMovementResidency::PendingTile {
                tile: TerrainTileIndex::new(22, 30).ok_or("neighbor tile")?,
            }
        );
        assert!(geometry.cached_bounds().is_none());
        assert!(geometry.triangles().is_empty());
        assert!(geometry.owner(0).is_none());
        assert!(geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.).is_err());
        // A copied contact identity survives candidate invalidation.
        assert!(matches!(
            owner,
            RuntimeMovementOwner::Static(RuntimeStaticMovementOwner::Terrain { .. })
        ));

        geometry.collect_interval(request)?;
        geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.)?;
        assert!(!geometry.triangles().is_empty());
        assert!(geometry.prepare_sweep(&volume, Vec3::X, f32::NAN).is_err());
        assert!(geometry.cached_bounds().is_none());
        assert!(geometry.triangles().is_empty());
        assert!(geometry.owner(0).is_none());
        geometry.collect_interval(request)?;
        geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.)?;
    }
    assert!(!query.triangles().is_empty());
    // New context/flags cannot inherit the preceding cache's ready state.
    {
        let mut geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0,
            MovementBspCacheMode::Enabled,
            &mut query,
        );
        assert!(geometry.cached_bounds().is_none());
        assert!(geometry.triangles().is_empty());
        geometry.collect_interval(request)?;
        geometry.prepare_sweep(&volume, Vec3::NEG_Z, 2.)?;
        assert!(geometry.triangles().is_empty());
    }
    Ok(())
}

#[test]
fn interval_collection_reaches_support_and_invalidates_incomplete_candidates()
-> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new(false)?;
    scene.synchronize()?;
    let position = Vec3::new(1000., 5800., 11.);
    let grounded = MovementIntervalRequest {
        position,
        radius: 0.5,
        height: 2.,
        distance: 0.,
        direction: Vec3::X,
        duration_ms: 1000,
        mode: MovementIntervalMode::Grounded(MovementGroundProfile::PlayerControlled {
            step_height: 1.,
        }),
    };
    let mut query = RuntimeMovementQuery::new();
    scene.terrain.collect_movement(
        &scene.world,
        &scene.objects,
        grounded.collection_bounds()?.body(),
        0x100111,
        MovementBspCacheMode::Enabled,
        &mut query,
    )?;
    assert!(
        query.triangles().is_empty(),
        "body-only collection cannot see support below the foot"
    );
    assert!(query.interval_bounds().is_none());
    assert_eq!(
        scene.terrain.collect_movement_interval(
            &scene.world,
            &scene.objects,
            grounded,
            0x100111,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(
        !query.triangles().is_empty(),
        "stationary ground probes must collect the lower floor"
    );
    assert_eq!(query.interval_bounds(), Some(grounded.collection_bounds()?));

    let trajectory = MovementFallTrajectory::new(MovementFallMode::Normal, 0.)?;
    let falling = MovementIntervalRequest {
        direction: Vec3::ZERO,
        mode: MovementIntervalMode::Airborne {
            fall_time_ms: 0,
            launch_height: position.z,
            trajectory,
        },
        ..grounded
    };
    assert_eq!(
        scene.terrain.collect_movement_interval(
            &scene.world,
            &scene.objects,
            falling,
            0x100111,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    let state = MovementFallState::new(MovementFallSnapshot {
        position,
        fall_time_ms: 0,
        launch_height: position.z,
        initial_downward_speed: 0.,
        horizontal_direction: Vec2::ZERO,
        horizontal_speed: 0.,
        direction: Vec3::ZERO,
        mode: MovementFallMode::Normal,
        phase: MovementFallPhase::Falling,
    })?;
    let result = state.advance(
        MovementFallInterval {
            duration_ms: 1000,
            displacement: Vec3::NEG_Z * trajectory.distance_at_millis(1000)?,
            radius: 0.5,
            height: 2.,
            support_profile: MovementSupportProfile::PlayerControlled,
            policy: MovementFallAdvancePolicy::Live,
        },
        query.triangles(),
    )?;
    let MovementFallContinuation::Landed {
        position: landed, ..
    } = result.continuation
    else {
        return Err("resident floor did not stop falling".into());
    };
    assert!((landed.z - 10.).abs() < 0.01, "{landed:?}");
    assert!(result.consumed_ms < 1000);
    assert!(matches!(
        query.owner(result.contact_triangle.ok_or("missing support identity")?),
        Some(RuntimeMovementOwner::Static(
            RuntimeStaticMovementOwner::Terrain { .. }
        ))
    ));

    // The body is still in the admitted tile, but private ground probes reach
    // a declared neighbor. Its absence must invalidate the whole candidate set.
    let edge = MovementIntervalRequest {
        position: Vec3::new(1000., 5334., 11.),
        direction: Vec3::NEG_Y,
        ..grounded
    };
    let first = TerrainTileIndex::new(21, 30).ok_or("first tile")?;
    assert!(
        edge.collection_bounds()?
            .body()
            .terrain_chunks()?
            .all(|(tile, _)| tile == first)
    );
    assert_eq!(
        scene.terrain.collect_movement_interval(
            &scene.world,
            &scene.objects,
            edge,
            0x100111,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::PendingTile {
            tile: TerrainTileIndex::new(22, 30).ok_or("neighbor tile")?
        }
    );
    assert!(query.triangles().is_empty());
    assert!(query.owner(0).is_none());
    assert!(query.map_id().is_none());
    assert!(query.interval_bounds().is_none());

    scene.terrain.collect_movement_interval(
        &scene.world,
        &scene.objects,
        falling,
        0x100111,
        MovementBspCacheMode::Enabled,
        &mut query,
    )?;
    assert!(!query.triangles().is_empty());
    assert!(
        scene
            .terrain
            .collect_movement_interval(
                &scene.world,
                &scene.objects,
                MovementIntervalRequest {
                    distance: f32::NAN,
                    ..falling
                },
                0x100111,
                MovementBspCacheMode::Enabled,
                &mut query
            )
            .is_err()
    );
    assert!(query.triangles().is_empty());
    assert!(query.map_id().is_none());
    assert!(query.interval_bounds().is_none());
    Ok(())
}
