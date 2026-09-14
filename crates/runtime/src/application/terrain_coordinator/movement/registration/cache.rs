//! Bounded sharing of exact point registrations with spatial invalidation.

use std::collections::{HashMap, VecDeque};

use glam::Vec3;
use solarity_systems::WorldModelRegistrationSelection;

use super::super::{RuntimeWorldModelMovementOwner, lighting::LightingChanges};

type Selection = WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>;

// Walking units must not retain every historical position. Eviction only repeats
// the native query; it never substitutes a nearby position or an older result.
const CAPACITY: usize = 2048;

/// Shares exact native selections while bounding position and revision history.
#[derive(Default)]
pub(in crate::application::terrain_coordinator::movement) struct UnitRegistrationCache {
    topology_revision: u64,
    entries: HashMap<[u32; 3], (u64, Selection)>,
    order: VecDeque<[u32; 3]>,
}

impl UnitRegistrationCache {
    /// Reuses only points whose geometry dependencies have remained unchanged.
    pub(in crate::application::terrain_coordinator::movement) fn get(
        &mut self,
        position: Vec3,
        changes: &LightingChanges,
    ) -> Option<Selection> {
        self.synchronize(changes);
        let (revision, selection) = self
            .entries
            .get_mut(&position.to_array().map(f32::to_bits))?;
        if changes.affects_unit(*revision, position) {
            return None;
        }
        // Advance even for disjoint motion, so a frequently used point does not
        // age out of the bounded motion history while an unrelated ship moves.
        *revision = changes.revision();
        Some(*selection)
    }

    /// Publishes a successful query and evicts the oldest point at capacity.
    pub(in crate::application::terrain_coordinator::movement) fn insert(
        &mut self,
        position: Vec3,
        changes: &LightingChanges,
        selection: Selection,
    ) {
        self.synchronize(changes);
        let key = position.to_array().map(f32::to_bits);
        if let Some(existing) = self.entries.get_mut(&key) {
            *existing = (changes.revision(), selection);
            return;
        }
        if self.entries.len() == CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (changes.revision(), selection));
        self.order.push_back(key);
    }

    /// Topology replacement invalidates even points outside moving-root bounds.
    fn synchronize(&mut self, changes: &LightingChanges) {
        if self.topology_revision != changes.topology_revision() {
            // Membership changes invalidate every point. Root motion uses the
            // same conservative XY dependency test as retained entity lighting.
            self.entries.clear();
            self.order.clear();
            self.topology_revision = changes.topology_revision();
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/unit_registration_cache.rs"]
mod tests;
