//! Explicit CPU ownership followed by queue-fenced GPU retirement.

mod accounting;
pub use accounting::GpuResourceUsage;
mod lease;
mod retirement;

pub use lease::GpuResourceLease;
use lease::{LeaseState, ResourceKey};
use retirement::ResourceRetirement;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Weak, mpsc},
};

/// A release notification is produced only at the last CPU owner drop. Frames
/// poll this cold queue; they never scan all resources or pin individual draws.
pub(super) struct ResourceLifetimes {
    sender: mpsc::Sender<ResourceKey>,
    receiver: mpsc::Receiver<ResourceKey>,
    owners: HashMap<ResourceKey, Weak<LeaseState>>,
    pending: VecDeque<ResourceRetirement>,
}

impl Default for ResourceLifetimes {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            owners: HashMap::new(),
            pending: VecDeque::new(),
        }
    }
}

impl ResourceLifetimes {
    /// Each registry resource shares one canonical lease across resident owners.
    fn pin(&mut self, key: ResourceKey) -> GpuResourceLease {
        if let Some(owner) = self.owners.get(&key).and_then(Weak::upgrade) {
            return GpuResourceLease(owner);
        }
        let owner = Arc::new(LeaseState {
            key,
            sender: self.sender.clone(),
        });
        self.owners.insert(key, Arc::downgrade(&owner));
        GpuResourceLease(owner)
    }
}
