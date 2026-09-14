//! Retained WMO doodad membership across unrelated dynamic model changes.

use std::collections::HashMap;

use crate::application::terrain_coordinator::RuntimeWorldModelMovementOwner;

pub(super) type Owner = (RuntimeWorldModelMovementOwner, usize);

/// Placement indices and first-owner light facts are the lookup's full dependency.
#[derive(Clone, Copy, Eq, PartialEq)]
struct Member {
    owner: Owner,
    index: usize,
    has_lights: bool,
}

/// Most unit/effect publications leave every static MODD entry unchanged. Compare
/// compact ordered membership before rebuilding the large root/doodad hash table.
#[derive(Default)]
pub(super) struct DoodadLookup {
    members: Vec<Member>,
    pending: Vec<Member>,
    index: HashMap<Owner, usize>,
    light_indices: Vec<usize>,
}

impl DoodadLookup {
    pub(super) fn begin(&mut self) {
        self.pending.clear();
    }

    pub(super) fn record(&mut self, owner: Owner, index: usize, has_lights: bool) {
        self.pending.push(Member {
            owner,
            index,
            has_lights,
        });
    }

    /// Duplicate keys retain the first placement, including its light ownership.
    /// A remap, removal, replacement or light change rebuilds the complete lookup.
    pub(super) fn finish(&mut self) {
        if self.pending == self.members {
            return;
        }
        std::mem::swap(&mut self.pending, &mut self.members);
        self.index.clear();
        self.light_indices.clear();
        for member in &self.members {
            if *self.index.entry(member.owner).or_insert(member.index) == member.index
                && member.has_lights
            {
                self.light_indices.push(member.index);
            }
        }
    }

    pub(super) fn index(&self) -> &HashMap<Owner, usize> {
        &self.index
    }

    pub(super) fn light_indices(&self) -> &[usize] {
        &self.light_indices
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/doodad_membership.rs"]
mod tests;
