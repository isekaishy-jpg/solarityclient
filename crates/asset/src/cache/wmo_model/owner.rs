//! Shared lifetime and path lookup for decoded build-12340 WMO generations.

use super::super::resource::{ResourceCache, ResourceLease};

use crate::{AssetError, AssetPath, AssetResourceKey, AssetStore, DecodedWorldModel};

/// Process-local shared cache of immutable WMO root and group generations.
///
/// One cache entry owns the root selected for a virtual path plus every
/// independently resolved numbered group. Scene placements retain that same
/// generation through [`ResourceLease`] instead of decoding it once per ADT reference.
#[derive(Default)]
pub struct WmoModelCache {
    models: ResourceCache<AssetResourceKey, DecodedWorldModel>,
}

impl WmoModelCache {
    /// Creates an empty cache without allocating its hash table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of root-WMO generations retained across namespaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Reports whether no decoded WMO generations are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Returns a shared WMO generation, loading its selected MPQ entries once.
    ///
    /// The namespace and normalized root path form the cache identity. Identically named
    /// HD roots and groups replace ordinary source bytes through archive
    /// precedence and never create a parallel quality-specific namespace.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when a cache miss cannot resolve or strictly
    /// decode the root and every numbered group. Failed loads are not retained.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<ResourceLease<DecodedWorldModel>, AssetError> {
        self.models.admit(store.model_cache_service().storage())?;
        let key = AssetResourceKey::new(store.namespace(), path.clone());
        if let Some(model) = self.models.get(&key)? {
            return Ok(model);
        }

        let model = DecodedWorldModel::load(store, path)?;
        self.models.insert(key, model)
    }

    /// Visits final-release notifications and returns the number of collected entries.
    ///
    /// Generations retained by resident placements survive collection. No
    /// guessed age, capacity, or HD-specific eviction policy is introduced.
    pub fn collect_unused(&mut self) -> usize {
        self.models.collect_unused()
    }
}
