//! Registration cache invalidation across moving roots and residency replacement.

use super::LightingChanges;
use glam::Vec3;

/// Fixed extents isolate XY registration overlap from changing root dimensions.
fn box_at(x: f32, y: f32) -> [Vec3; 2] {
    [Vec3::new(x, y, -5.), Vec3::new(x + 10., y + 10., 5.)]
}

/// Both movement endpoints and inclusive box contacts must invalidate probes.
#[test]
fn root_motion_invalidates_arrivals_departures_and_boundary_contacts() {
    let mut changes = LightingChanges::default();
    let previous = changes.revision();
    let topology = changes.topology_revision();
    changes.moved(box_at(0., 0.), box_at(20., 0.));
    for position in [
        Vec3::new(5., 5., 0.),
        Vec3::new(25., 5., 0.),
        Vec3::new(30., 10., 900.),
        Vec3::new(0., 0., -900.),
    ] {
        assert!(changes.affects_unit(previous, position));
    }
    assert!(!changes.affects_unit(previous, Vec3::new(5., 20., 0.)));
    assert!(!changes.affects_unit(previous, Vec3::new(40., 5., 0.)));
    assert_eq!(changes.topology_revision(), topology);
    assert!(!changes.affects_unit(changes.revision(), Vec3::ZERO));
}

/// Skipped consumers still observe every root movement after their revision.
#[test]
fn retained_queries_consider_every_motion_since_their_last_frame() {
    let mut changes = LightingChanges::default();
    let first = changes.revision();
    changes.moved(box_at(0., 0.), box_at(1., 0.));
    let second = changes.revision();
    changes.moved(box_at(100., 100.), box_at(101., 100.));
    assert!(changes.affects_unit(first, Vec3::ZERO));
    assert!(!changes.affects_unit(second, Vec3::ZERO));
    assert!(changes.affects_unit(second, Vec3::new(105., 105., 0.)));
}

/// An incomplete dependency history must execute the original spatial query.
#[test]
fn topology_replacement_and_expired_history_require_the_original_query() {
    let mut changes = LightingChanges::default();
    let first = changes.revision();
    changes.moved(box_at(0., 0.), box_at(1., 0.));
    changes.invalidate();
    changes.moved(box_at(200., 200.), box_at(201., 200.));
    assert!(changes.affects_unit(first, Vec3::splat(-1000.)));
    assert_eq!(changes.topology_revision(), 1);
    let retained = changes.revision();
    for _ in 0..64 {
        changes.moved(box_at(0., 0.), box_at(1., 0.));
    }
    assert!(!changes.affects_unit(retained, Vec3::splat(-1000.)));
    changes.moved(box_at(0., 0.), box_at(1., 0.));
    assert!(changes.affects_unit(retained, Vec3::splat(-1000.)));
}

/// Wrapping revision arithmetic preserves the immediately preceding dependency.
#[test]
fn revision_wrap_preserves_recent_motion_dependencies() {
    let mut changes = LightingChanges {
        revision: u64::MAX,
        ..Default::default()
    };
    changes.moved(box_at(0., 0.), box_at(1., 0.));
    assert!(changes.affects_unit(u64::MAX, Vec3::ZERO));
    assert!(!changes.affects_unit(u64::MAX, Vec3::splat(-1000.)));
}
