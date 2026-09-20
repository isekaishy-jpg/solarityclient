//! Fixture access to ordered storage without enlarging the production facade.

use crate::application::terrain_frame::m2::placement_fixtures as fixtures;

use super::M2GpuPlacementOwner;
use super::{M2GpuPlacement, M2PlacementStorage};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Owner;

impl M2PlacementStorage {
    pub(in crate::application::terrain_frame::m2) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Fixture teardown discards both simulation and its publication identity.
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        self.entries.clear();
        self.lineage.clear();
        self.dynamic_indices.clear();
        self.owners.clear();
        self.static_layout_dirty = true;
    }

    /// Explicit single-owner fixture removal preserves survivor lineage.
    pub(in crate::application::terrain_frame::m2) fn remove(
        &mut self,
        index: usize,
    ) -> M2GpuPlacement {
        self.extract_from(index, |candidate, _| candidate == index)
            .pop()
            .unwrap_or_else(|| unreachable!("fixture selected one existing placement"))
            .1
    }

    pub(in crate::application::terrain_frame::m2) fn last(&self) -> Option<&M2GpuPlacement> {
        self.entries.last()
    }

    pub(in crate::application::terrain_frame::m2) fn last_mut(
        &mut self,
    ) -> Option<&mut M2GpuPlacement> {
        self.entries.last_mut()
    }
}

/// Full publication alone acknowledges static changes; dynamic/effect publication
/// cannot hide a pending relocation, removal or replacement of static identity.
#[test]
fn placement_journal_tracks_static_layout_and_ordered_dynamic_membership() {
    let mut storage = M2PlacementStorage::from(fixtures::placement_fixture(20, 8));
    assert!(!storage.static_layout_unchanged());
    storage.published_from(0);
    assert!(storage.static_layout_unchanged());
    storage.retain_dynamic(|placement| {
        !matches!(placement.owner, M2GpuPlacementOwner::CreatureMount { .. })
    });
    assert!(storage.static_layout_unchanged());
    storage.push(fixtures::placement(M2GpuPlacementOwner::CreatureBody {
        guid: 100,
    }));
    storage.published_dynamic();
    assert!(storage.static_layout_unchanged());
    assert_journal(&storage);

    // Append scenery after dynamics, then remove an earlier dynamic owner.
    storage.push(fixtures::placement(M2GpuPlacementOwner::Static(
        ResidentM2Owner::TerrainDoodad { unique_id: 99 },
    )));
    assert!(!storage.static_layout_unchanged());
    storage.published_from(0);
    storage.retain_dynamic(|placement| {
        !matches!(
            placement.owner,
            M2GpuPlacementOwner::CreatureBody { guid: 100 }
        )
    });
    assert!(
        !storage.static_layout_unchanged(),
        "compaction moved later scenery"
    );
    storage.published_from(storage.len());
    assert!(
        !storage.static_layout_unchanged(),
        "empty tail publication cannot acknowledge a moved static owner"
    );
    assert_journal(&storage);
    storage.published_from(0);
    storage.extract_from(0, |_, placement| {
        matches!(
            placement.owner,
            M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad { unique_id: 0 })
        )
    });
    assert!(!storage.static_layout_unchanged());
    assert_journal(&storage);
}

/// Effect ordering moves only the suffix after the first effect, and it must
/// still invalidate static indices if newly admitted scenery lies in that suffix.
#[test]
fn effect_partition_updates_journal_without_visiting_the_static_prefix() {
    let mut storage = M2PlacementStorage::from(fixtures::placement_fixture(100, 0));
    for guid in 1..=4 {
        storage.push(fixtures::placement(M2GpuPlacementOwner::CreatureBody {
            guid,
        }));
        // This fixture tests the storage partition's flags, not effect playback.
        let last = storage.lineage.len() - 1;
        storage.lineage[last].is_effect = guid % 2 == 1;
    }
    storage.published_from(0);
    storage.order_effects_last();
    assert!(storage.static_layout_unchanged());
    assert_eq!(
        storage.entries[100..]
            .iter()
            .map(|placement| placement.owner)
            .collect::<Vec<_>>(),
        [2, 4, 1, 3].map(|guid| M2GpuPlacementOwner::CreatureBody { guid })
    );
    assert_journal(&storage);
    storage.push(fixtures::placement(M2GpuPlacementOwner::Static(
        ResidentM2Owner::TerrainDoodad { unique_id: 101 },
    )));
    storage.published_from(0);
    storage.order_effects_last();
    assert!(!storage.static_layout_unchanged());
    assert!(storage.lineage[102].is_static);
    assert_journal(&storage);
}

/// Structural operations publish exactly the current native-order dynamic set.
fn assert_journal(storage: &M2PlacementStorage) {
    assert_eq!(
        storage.dynamic_indices(),
        storage
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, placement)| (!matches!(
                placement.owner,
                M2GpuPlacementOwner::Static(_)
            ))
            .then_some(index))
            .collect::<Vec<_>>()
    );
    for &index in storage.dynamic_indices() {
        let owner = storage.entries[index].owner;
        assert_eq!(
            storage.owner_indices(owner).collect::<Vec<_>>(),
            storage
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, placement)| (placement.owner == owner).then_some(index))
                .collect::<Vec<_>>()
        );
    }
}

/// Owner lookup remains current before rendering publishes, including duplicate
/// owners, in-place retirement and a new lifetime reusing the same gameplay ID.
#[test]
fn owners_follow_mutations_before_render_publication() {
    let mut storage = M2PlacementStorage::from(fixtures::placement_fixture(2048, 10));
    storage.published_from(0);
    let body = M2GpuPlacementOwner::CreatureBody { guid: 5 };
    let duplicate = storage.owner_indices(body).collect::<Vec<_>>();
    assert_eq!(duplicate.len(), 2);
    let replacement = M2GpuPlacementOwner::CreatureBody { guid: 100 };
    storage.rename_dynamic_owner(duplicate[0], replacement);
    assert_eq!(
        storage.owner_indices(body).collect::<Vec<_>>(),
        [duplicate[1]]
    );
    storage.rename_dynamic_owner(duplicate[1], replacement);
    assert!(storage.owner_indices(body).next().is_none());
    assert_eq!(
        storage.owner_indices(replacement).collect::<Vec<_>>(),
        duplicate
    );
    storage.push(fixtures::placement(body));
    assert_eq!(storage.owner_indices(body).next(), Some(storage.len() - 1));
    storage.rename_dynamic_owner(duplicate[0], body);
    assert_eq!(storage.owner_indices(body).next(), Some(duplicate[0]));
    storage.rename_dynamic_owner(duplicate[1], body);
    assert!(storage.owner_indices(replacement).next().is_none());
    assert_journal(&storage);
    let removed = storage.extract_from(duplicate[0], |index, _| index == duplicate[0]);
    assert_eq!(removed.len(), 1);
    assert_journal(&storage);
    storage.retain_dynamic(|placement| placement.owner != body);
    assert!(storage.owner_indices(body).next().is_none());
    assert_journal(&storage);
}
