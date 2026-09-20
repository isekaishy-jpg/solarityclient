//! Current simulation ownership is independent of deferred rendering metadata.

use super::{M2GpuPlacementOwner, M2PlacementStorage, PlacementLineage};

/// Traverses only matching dynamic owners in native placement order. Duplicate
/// owners retain first-match and predicate-search behavior without extra vectors.
pub(in super::super) struct OwnerIndices<'a> {
    lineage: &'a [PlacementLineage],
    next: Option<usize>,
}

impl Iterator for OwnerIndices<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.next?;
        self.next = self.lineage[index].next_owner;
        Some(index)
    }
}

impl M2PlacementStorage {
    /// Owners are indexed at membership changes, before any renderer publication.
    pub(in super::super) fn owner_indices(&self, owner: M2GpuPlacementOwner) -> OwnerIndices<'_> {
        debug_assert!(!matches!(owner, M2GpuPlacementOwner::Static(_)));
        OwnerIndices {
            lineage: &self.lineage,
            next: self.owners.get(&owner).map(|&(first, _)| first),
        }
    }

    /// Reuses lookup capacity and visits only dynamic members after compaction.
    pub(super) fn rebuild_owners(&mut self) {
        self.owners.clear();
        for ordinal in 0..self.dynamic_indices.len() {
            self.link_owner(self.dynamic_indices[ordinal]);
        }
    }

    /// Appends a record after every existing member of its owner family.
    pub(super) fn link_owner(&mut self, index: usize) {
        self.lineage[index].next_owner = None;
        let owner = self.entries[index].owner;
        let family = self.owners.entry(owner).or_insert((index, index));
        if family.1 != index {
            self.lineage[family.1].next_owner = Some(index);
            family.1 = index;
        }
    }

    /// Retirement renames a living instance in place. Update both families
    /// immediately, preserving the native order even when an owner is repeated.
    pub(in super::super) fn rename_dynamic_owner(
        &mut self,
        index: usize,
        owner: M2GpuPlacementOwner,
    ) {
        debug_assert!(!self.lineage[index].is_static);
        debug_assert!(!matches!(owner, M2GpuPlacementOwner::Static(_)));
        let previous = self.entries[index].owner;
        if previous == owner {
            return;
        }
        let (first, last) = self.owners[&previous];
        let next = self.lineage[index].next_owner;
        if first == index {
            if let Some(next) = next {
                self.owners.insert(previous, (next, last));
            } else {
                self.owners.remove(&previous);
            }
        } else {
            let predecessor = self
                .owner_indices(previous)
                .find(|&candidate| self.lineage[candidate].next_owner == Some(index))
                .unwrap_or_else(|| unreachable!("indexed owner has a preceding family member"));
            self.lineage[predecessor].next_owner = next;
            if last == index {
                self.owners.insert(previous, (first, predecessor));
            }
        }
        self.entries[index].owner = owner;
        let Some(&(first, last)) = self.owners.get(&owner) else {
            self.lineage[index].next_owner = None;
            self.owners.insert(owner, (index, index));
            return;
        };
        if index < first {
            self.lineage[index].next_owner = Some(first);
            self.owners.insert(owner, (index, last));
            return;
        }
        let predecessor = self
            .owner_indices(owner)
            .take_while(|&candidate| candidate < index)
            .last()
            .unwrap_or_else(|| unreachable!("first owner precedes renamed placement"));
        self.lineage[index].next_owner = self.lineage[predecessor].next_owner;
        self.lineage[predecessor].next_owner = Some(index);
        if index > last {
            self.owners.insert(owner, (first, index));
        }
    }
}
