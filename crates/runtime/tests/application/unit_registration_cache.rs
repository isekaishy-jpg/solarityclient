//! Repeated consumers share a point without retaining a walking unit's history.

use super::*;
use crate::application::terrain_coordinator::movement::lighting::LightingChanges;
use solarity_systems::WorldModelRegistrationQuery;

#[test]
fn registration_sharing_requires_exact_position_and_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = UnitRegistrationCache::default();
    let mut changes = LightingChanges::default();
    let point = Vec3::new(1., 2., 3.);
    let selection = WorldModelRegistrationQuery::new(point, point - Vec3::Z, point)?.finish();
    cache.insert(point, &changes, selection);
    for _ in 0..4 {
        assert_eq!(cache.get(point, &changes), Some(selection));
    }
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        assert!(cache.get(point + axis * 0.00001, &changes).is_none());
    }
    changes.moved([Vec3::ZERO, Vec3::ONE], [point, point + Vec3::ONE]);
    assert!(cache.get(point, &changes).is_none());
    cache.insert(point, &changes, selection);
    changes.invalidate();
    assert!(cache.get(point, &changes).is_none());
    Ok(())
}

#[test]
fn moving_positions_remain_bounded_and_evicted_points_recompute()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = UnitRegistrationCache::default();
    let changes = LightingChanges::default();
    let selection = WorldModelRegistrationQuery::new(Vec3::Z, -Vec3::Z, Vec3::ZERO)?.finish();
    for index in 0..CAPACITY * 4 {
        let point = Vec3::new(index as f32, 0., 0.);
        cache.insert(point, &changes, selection);
        cache.insert(point, &changes, selection);
        assert_eq!(cache.get(point, &changes), Some(selection));
        assert!(cache.entries.len() <= CAPACITY);
        assert_eq!(cache.order.len(), cache.entries.len());
    }
    assert!(cache.get(Vec3::ZERO, &changes).is_none());
    cache.insert(Vec3::ZERO, &changes, selection);
    assert_eq!(cache.get(Vec3::ZERO, &changes), Some(selection));
    Ok(())
}

#[test]
fn unrelated_root_motion_preserves_queries_without_aging_out_active_points()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cache = UnitRegistrationCache::default();
    let mut changes = LightingChanges::default();
    let selection = WorldModelRegistrationQuery::new(Vec3::Z, -Vec3::Z, Vec3::ZERO)?.finish();
    cache.insert(Vec3::ZERO, &changes, selection);
    cache.insert(Vec3::X, &changes, selection);
    let bounds = [Vec3::splat(100.), Vec3::splat(101.)];
    for _ in 0..200 {
        changes.moved(bounds, bounds);
        assert_eq!(cache.get(Vec3::ZERO, &changes), Some(selection));
    }
    // An inactive point older than the available history runs the native query.
    assert!(cache.get(Vec3::X, &changes).is_none());
    changes.moved(bounds, [Vec3::splat(-1.), Vec3::ONE]);
    assert!(cache.get(Vec3::ZERO, &changes).is_none());
    Ok(())
}
