//! Topology-owned request groups; frame consumers borrow only their owner's slice.

use solarity_rendering::CharacterAttachmentPoint;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// The parent identity selecting one ordered group of attachment requests.
pub(in crate::application::terrain_frame::m2) trait AttachmentRequest:
    Copy
{
    type Owner: Copy + Eq + Hash;

    fn owner(self) -> Self::Owner;
}

impl AttachmentRequest for (u64, CharacterAttachmentPoint) {
    type Owner = u64;

    fn owner(self) -> u64 {
        self.0
    }
}

impl AttachmentRequest for (u64, CharacterAttachmentPoint, u32) {
    type Owner = (u64, CharacterAttachmentPoint);

    fn owner(self) -> Self::Owner {
        (self.0, self.1)
    }
}

/// Group storage changes only with topology, never while a frame borrows it.
pub(in crate::application::terrain_frame::m2) struct AttachmentRequests<R: AttachmentRequest> {
    groups: HashMap<R::Owner, Vec<R>>,
    count: usize,
}

impl<R: AttachmentRequest> Default for AttachmentRequests<R> {
    fn default() -> Self {
        Self {
            groups: HashMap::new(),
            count: 0,
        }
    }
}

impl<R: AttachmentRequest> AttachmentRequests<R> {
    /// Reuses surviving owners' storage and releases departed owners' groups.
    /// Input order and duplicates within each owner are observable and retained.
    pub(in crate::application::terrain_frame::m2) fn replace(
        &mut self,
        requests: impl IntoIterator<Item = R>,
    ) {
        for group in self.groups.values_mut() {
            group.clear();
        }
        self.count = 0;
        for request in requests {
            self.groups
                .entry(request.owner())
                .or_default()
                .push(request);
            self.count += 1;
        }
        self.groups.retain(|_, group| !group.is_empty());
    }

    pub(in crate::application::terrain_frame::m2) fn for_owner(&self, owner: R::Owner) -> &[R] {
        self.groups.get(&owner).map_or(&[], Vec::as_slice)
    }

    pub(in crate::application::terrain_frame::m2) fn len(&self) -> usize {
        self.count
    }
}

impl<R: AttachmentRequest> FromIterator<R> for AttachmentRequests<R> {
    fn from_iter<T: IntoIterator<Item = R>>(iter: T) -> Self {
        let mut requests = Self::default();
        requests.replace(iter);
        requests
    }
}

/// Constant-time mount membership with the original count, including duplicates.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct MountedOwners {
    owners: HashSet<u64>,
    count: usize,
}

impl MountedOwners {
    /// Replaces topology membership without discarding the retained allocation.
    pub(in crate::application::terrain_frame::m2) fn replace(
        &mut self,
        owners: impl IntoIterator<Item = u64>,
    ) {
        self.owners.clear();
        self.count = 0;
        for owner in owners {
            self.owners.insert(owner);
            self.count += 1;
        }
    }

    pub(in crate::application::terrain_frame::m2) fn contains(&self, owner: &u64) -> bool {
        self.owners.contains(owner)
    }

    pub(in crate::application::terrain_frame::m2) fn len(&self) -> usize {
        self.count
    }
}
