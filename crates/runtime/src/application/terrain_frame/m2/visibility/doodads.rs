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
    static_light_indices: Vec<usize>,
    dynamic_members: Vec<Member>,
    /// A partial dynamic publication invalidates the old complete comparison.
    members_valid: bool,
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
        if self.members_valid && self.pending == self.members {
            return;
        }
        std::mem::swap(&mut self.pending, &mut self.members);
        self.index.clear();
        self.light_indices.clear();
        self.static_light_indices.clear();
        self.dynamic_members.clear();
        for member in &self.members {
            let dynamic = matches!(
                member.owner.0,
                RuntimeWorldModelMovementOwner::GameObject { .. }
            );
            if dynamic {
                self.dynamic_members.push(*member);
            }
            if *self.index.entry(member.owner).or_insert(member.index) == member.index
                && member.has_lights
            {
                self.light_indices.push(member.index);
                if !dynamic {
                    self.static_light_indices.push(member.index);
                }
            }
        }
        self.members_valid = true;
    }

    /// Only replicated-WMO keys change when static placement indices survive.
    /// Their owner domain cannot collide with a resident static WMO key. Preserve
    /// first occurrence and native light order without walking static MODD entries.
    pub(super) fn finish_dynamic(&mut self) {
        debug_assert!(self.pending.iter().all(|member| matches!(
            member.owner.0,
            RuntimeWorldModelMovementOwner::GameObject { .. }
        )));
        if self.pending == self.dynamic_members {
            return;
        }
        for member in &self.dynamic_members {
            self.index.remove(&member.owner);
        }
        self.light_indices.clone_from(&self.static_light_indices);
        for member in &self.pending {
            if *self.index.entry(member.owner).or_insert(member.index) == member.index
                && member.has_lights
            {
                self.light_indices.push(member.index);
            }
        }
        self.light_indices.sort_unstable();
        self.dynamic_members.clone_from(&self.pending);
        self.members_valid = false;
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
