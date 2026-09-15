//! Explicit immutable consumer leases and coalesced cache-release ownership.

mod clock;
mod index;
mod lease;
mod releases;

pub use clock::ResourceCacheClock;
pub(crate) use index::ResourceCache;
pub use lease::{ResourceLease, ResourceWeak};
