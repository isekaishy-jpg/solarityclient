//! Membership mutations carry publication lineage in the same stable order.

use super::{M2GpuPlacement, M2PlacementStorage, PlacementLineage};

impl M2PlacementStorage {
    /// New records have no published metadata, even when their owner is reused.
    pub(in super::super) fn push(&mut self, placement: M2GpuPlacement) {
        let lineage = PlacementLineage::new(&placement);
        if lineage.is_static {
            self.static_layout_dirty = true;
        } else {
            self.dynamic_indices.push(self.entries.len());
        }
        self.lineage.push(lineage);
        self.entries.push(placement);
    }

    /// Retains native scene order and the old metadata index of each survivor.
    pub(in super::super) fn retain(&mut self, mut keep: impl FnMut(&M2GpuPlacement) -> bool) {
        self.retain_where(|_, placement| keep(placement));
    }

    /// Unit and effect retirement cannot remove immutable scenery. Consult the
    /// compact classification before touching a simulation record or its owner.
    pub(in super::super) fn retain_dynamic(
        &mut self,
        mut keep: impl FnMut(&M2GpuPlacement) -> bool,
    ) {
        self.retain_where(|slot, placement| slot.is_static || keep(placement));
    }

    /// Structural filtering owns the aligned move for both membership domains.
    fn retain_where(&mut self, mut keep: impl FnMut(&PlacementLineage, &M2GpuPlacement) -> bool) {
        let mut read = 0;
        let mut write = 0;
        self.dynamic_indices.clear();
        self.entries.retain(|placement| {
            let slot = self.lineage[read];
            let retained = keep(&slot, placement);
            self.static_layout_dirty |= slot.is_static && (!retained || write != read);
            if retained {
                if !slot.is_static {
                    self.dynamic_indices.push(write);
                }
                if write != read {
                    self.lineage[write] = slot;
                }
                write += 1;
            }
            read += 1;
            retained
        });
        self.lineage.truncate(write);
    }

    /// Transfers selected instances without rebuilding storage for the scenery
    /// prefix. Returned indices refer to the scene before this extraction.
    pub(in super::super) fn extract_from(
        &mut self,
        first: usize,
        mut remove: impl FnMut(usize, &mut M2GpuPlacement) -> bool,
    ) -> Vec<(usize, M2GpuPlacement)> {
        let mut read = first;
        let mut write = first;
        let mut removed_indices = Vec::new();
        let retained_dynamic = self.dynamic_indices.partition_point(|index| *index < first);
        self.dynamic_indices.truncate(retained_dynamic);
        let removed = self
            .entries
            .extract_if(first.., |placement| {
                let removed = remove(read, placement);
                let slot = self.lineage[read];
                self.static_layout_dirty |= slot.is_static && (removed || write != read);
                if removed {
                    removed_indices.push(read);
                } else {
                    if !slot.is_static {
                        self.dynamic_indices.push(write);
                    }
                    if write != read {
                        self.lineage[write] = slot;
                    }
                    write += 1;
                }
                read += 1;
                removed
            })
            .collect::<Vec<_>>();
        self.lineage.truncate(write);
        removed_indices.into_iter().zip(removed).collect()
    }

    /// Stable-partitions from compact flags. Already ordered scenery does not
    /// need a sorting traversal through the large animation/effect records.
    pub(in super::super) fn order_effects_last(&mut self) {
        let Some(first) = self
            .dynamic_indices
            .iter()
            .copied()
            .find(|&index| self.lineage[index].is_effect)
        else {
            return;
        };
        if self.lineage[first..].iter().all(|slot| slot.is_effect) {
            return;
        }
        let ordinary = self.lineage[first..]
            .iter()
            .filter(|slot| !slot.is_effect)
            .count();
        let mut next_ordinary = 0;
        let mut next_effect = ordinary;
        let mut destinations = self
            .lineage
            .iter()
            .skip(first)
            .map(|slot| {
                let next = if slot.is_effect {
                    &mut next_effect
                } else {
                    &mut next_ordinary
                };
                let destination = *next;
                *next += 1;
                destination
            })
            .collect::<Vec<_>>();
        for index in 0..destinations.len() {
            while destinations[index] != index {
                let destination = destinations[index];
                self.static_layout_dirty |= self.lineage[first + index].is_static
                    || self.lineage[first + destination].is_static;
                self.entries.swap(first + index, first + destination);
                self.lineage.swap(first + index, first + destination);
                destinations.swap(index, destination);
            }
        }
        let retained = self.dynamic_indices.partition_point(|index| *index < first);
        self.dynamic_indices.truncate(retained);
        self.dynamic_indices.extend(
            self.lineage
                .iter()
                .enumerate()
                .skip(first)
                .filter_map(|(index, slot)| (!slot.is_static).then_some(index)),
        );
    }
}

impl Extend<M2GpuPlacement> for M2PlacementStorage {
    fn extend<T: IntoIterator<Item = M2GpuPlacement>>(&mut self, entries: T) {
        for placement in entries {
            self.push(placement);
        }
    }
}
