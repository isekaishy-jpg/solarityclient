//! Dependency revisions for retained native floor and liquid registrations.

use std::collections::VecDeque;

use glam::Vec3;

/// Motion invalidates only unit probes crossing the old or new root box. Root
/// membership and terrain publication invalidate every registration. The bounded
/// history is an optimization: older consumers simply run the original query.
#[derive(Default)]
pub(super) struct LightingChanges {
    revision: u64,
    topology_revision: u64,
    motions: VecDeque<[Vec3; 2]>,
}

impl LightingChanges {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn topology_revision(&self) -> u64 {
        self.topology_revision
    }

    /// A source, destination list, or terrain generation can change any result.
    pub(super) fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        self.motions.clear();
    }

    /// Includes departure and arrival, so leaving a cached floor invalidates it
    /// just as entering a previously empty registration does.
    pub(super) fn moved(&mut self, previous: [Vec3; 2], current: [Vec3; 2]) {
        self.revision = self.revision.wrapping_add(1);
        if self.motions.len() == 64 {
            self.motions.pop_front();
        }
        self.motions
            .push_back([previous[0].min(current[0]), previous[1].max(current[1])]);
    }

    /// Unit_C's 7C2A70 registration probes vertical segments at the unit XY,
    /// including the upward probe. Both directions first test the world root box
    /// (7C25D0). Ignoring Z here also covers every native segment endpoint.
    /// MapObject box registration is deliberately not covered by this predicate.
    pub(super) fn affects_unit(&self, previous: u64, position: Vec3) -> bool {
        let count = self.revision.wrapping_sub(previous);
        if count > self.motions.len() as u64 {
            return true;
        }
        self.motions
            .iter()
            .rev()
            .take(count as usize)
            .any(|bounds| {
                position.x >= bounds[0].x
                    && position.x <= bounds[1].x
                    && position.y >= bounds[0].y
                    && position.y <= bounds[1].y
            })
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/lighting_changes.rs"]
mod tests;
