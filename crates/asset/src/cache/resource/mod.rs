//! Explicit immutable consumer leases and coalesced cache-release ownership.

mod index;
mod lease;
mod releases;

pub(crate) use index::ResourceCache;
pub use lease::{ResourceLease, ResourceWeak};
