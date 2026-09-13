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
