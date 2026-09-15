//! Qualified M2 resource retention and namespace-owned worker maintenance.

mod maintenance;
mod owner;
mod requests;

pub use maintenance::{M2CacheCollection, M2CacheService};
pub use requests::{M2Load, M2LoadError, M2LoadProducer, M2LoadRequest};

use super::resource::ResourceCache;
use crate::{AssetNamespaceId, AssetResourceKey, DecodedM2Model};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

/// The namespace service pins this core until cleanup runs outside presentation.
struct ModelCacheCore {
    models: Mutex<ResourceCache<AssetResourceKey, DecodedM2Model>>,
    owned: AtomicBool,
}

/// Shared immutable M2/SKIN sources with stock's qualified ten-second release grace.
/// Lookup membership qualifies these entries. Standalone ResourceLease values do
/// not enter this cache and retain immediate final-consumer destruction.
pub struct M2ModelCache {
    core: Arc<ModelCacheCore>,
    namespaces: HashSet<AssetNamespaceId>,
}
