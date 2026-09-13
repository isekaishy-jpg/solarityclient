//! Stable quad allocations separate physical storage from UI presentation order.

use std::collections::{BTreeMap, HashMap};

/// Reuses vacant spans and keeps spare capacity with its live source owner.
/// Superseded spans are coalesced, so repeated text changes retain useful space
/// rather than appending a new historical copy on every publication.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct QuadStorage {
    capacities: HashMap<usize, usize>,
    free: BTreeMap<usize, usize>,
}

impl QuadStorage {
    /// Selects the span after callers have validated all candidate vertices.
    /// Initial packed runs have exact capacity until their first resize.
    pub(super) fn replace(
        &mut self,
        old: Option<(usize, usize)>,
        count: usize,
        extent: usize,
    ) -> Result<(usize, usize), super::UiMeshPlanError> {
        let old =
            old.map(|(start, used)| (start, self.capacities.get(&start).copied().unwrap_or(used)));
        if let Some((start, capacity)) = old
            && count != 0
            && count <= capacity
        {
            self.capacities.insert(start, capacity);
            return Ok((start, capacity));
        }
        let capacity = if count == 0 {
            0
        } else {
            count
                .checked_next_power_of_two()
                .ok_or(super::UiMeshPlanError::Capacity { domain: "quad" })?
                .min(u32::MAX as usize / 4)
                .max(count)
        };
        let vacant = self
            .free
            .iter()
            .find(|(_, available)| **available >= capacity)
            .map(|(&start, &available)| (start, available));
        let start = vacant.map_or(extent, |(start, _)| start);
        if start
            .checked_add(capacity)
            .is_none_or(|end| end > u32::MAX as usize / 4)
        {
            return Err(super::UiMeshPlanError::Capacity { domain: "vertex" });
        }
        if count != 0 {
            if let Some((start, available)) = vacant {
                self.free.remove(&start);
                if available > capacity {
                    self.free.insert(start + capacity, available - capacity);
                }
            }
            self.capacities.insert(start, capacity);
        }
        if let Some((old_start, old_capacity)) = old {
            self.capacities.remove(&old_start);
            self.release(old_start, old_capacity);
        }
        Ok((start, capacity))
    }

    /// Adjacent vacancies form one reusable allocation regardless of their owners.
    fn release(&mut self, mut start: usize, mut count: usize) {
        if let Some((&before, &length)) = self.free.range(..start).next_back()
            && before + length == start
        {
            self.free.remove(&before);
            start = before;
            count += length;
        }
        if let Some(length) = self.free.remove(&(start + count)) {
            count += length;
        }
        self.free.insert(start, count);
    }
}
