//! Ordered MODD lookup survives unrelated publication and exact membership changes.

use super::{DoodadLookup, RuntimeWorldModelMovementOwner};

/// Duplicate root/index keys keep the first placement's light ownership.
#[test]
fn doodad_membership_preserves_first_owner_and_rebuilds_remapped_lights() {
    let root = RuntimeWorldModelMovementOwner::Static { unique_id: 17 };
    let first = (root, 2);
    let second = (root, 3);
    let mut lookup = DoodadLookup::default();
    for _ in 0..3 {
        lookup.begin();
        lookup.record(first, 20, false);
        lookup.record(first, 30, true);
        lookup.record(second, 40, true);
        lookup.finish();
        assert_eq!(lookup.index().get(&first), Some(&20));
        assert_eq!(lookup.index().get(&second), Some(&40));
        assert_eq!(lookup.light_indices(), &[40]);
    }
    lookup.begin();
    lookup.record(first, 10, true);
    lookup.record(second, 15, false);
    lookup.finish();
    assert_eq!(lookup.index().get(&first), Some(&10));
    assert_eq!(lookup.index().get(&second), Some(&15));
    assert_eq!(lookup.light_indices(), &[10]);
    lookup.begin();
    lookup.record(second, 15, true);
    lookup.finish();
    assert_eq!(lookup.index().get(&first), None);
    assert_eq!(lookup.light_indices(), &[15]);
    lookup.begin();
    lookup.finish();
    assert!(lookup.index().is_empty());
    assert!(lookup.light_indices().is_empty());
}

/// Static and replicated WMO identities are disjoint; partial updates preserve
/// static keys and native light order, including a return to an older full set.
#[test]
fn dynamic_doodads_preserve_static_members_and_invalidate_old_full_comparisons()
-> Result<(), Box<dyn std::error::Error>> {
    let world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(1),
        7,
        "Fixture",
        glam::Vec3::ZERO,
        0.,
    ));
    let dynamic = (
        RuntimeWorldModelMovementOwner::GameObject {
            identity: world.object_identity(7).ok_or("identity")?,
        },
        0,
    );
    let fixed = (RuntimeWorldModelMovementOwner::Static { unique_id: 17 }, 0);
    let mut lookup = DoodadLookup::default();
    let full = |lookup: &mut DoodadLookup| {
        lookup.begin();
        lookup.record(dynamic, 5, false);
        lookup.record(fixed, 10, true);
        lookup.record(dynamic, 20, true);
        lookup.finish();
    };
    full(&mut lookup);
    assert_eq!(lookup.light_indices(), &[10]);
    lookup.begin();
    lookup.record(dynamic, 4, true);
    lookup.finish_dynamic();
    assert_eq!(lookup.index().get(&fixed), Some(&10));
    assert_eq!(lookup.index().get(&dynamic), Some(&4));
    assert_eq!(lookup.light_indices(), &[4, 10]);
    lookup.begin();
    lookup.finish_dynamic();
    assert_eq!(lookup.index().get(&dynamic), None);
    assert_eq!(lookup.light_indices(), &[10]);
    full(&mut lookup);
    assert_eq!(lookup.index().get(&dynamic), Some(&5));
    assert_eq!(lookup.light_indices(), &[10]);
    Ok(())
}
