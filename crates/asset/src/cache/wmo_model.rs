//! Shared lifetime and path lookup for decoded build-12340 WMO generations.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{AssetError, AssetPath, AssetStore, DecodedWorldModel};

/// Process-local shared cache of immutable WMO root and group generations.
///
/// One cache entry owns the root selected for a virtual path plus every
/// independently resolved numbered group. Scene placements retain that same
/// generation through [`Arc`] instead of decoding it once per ADT reference.
#[derive(Default)]
pub struct WmoModelCache {
    models: HashMap<AssetPath, Arc<DecodedWorldModel>>,
}

impl WmoModelCache {
    /// Creates an empty cache without allocating its hash table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of distinct normalized root-WMO paths retained.
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
    /// The normalized root path is the only cache identity. Identically named
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
    ) -> Result<Arc<DecodedWorldModel>, AssetError> {
        if let Some(model) = self.models.get(path) {
            return Ok(Arc::clone(model));
        }

        let model = Arc::new(DecodedWorldModel::load(store, path)?);
        self.models.insert(path.clone(), Arc::clone(&model));
        Ok(model)
    }

    /// Releases entries held only by the cache and returns the removal count.
    ///
    /// Generations retained by resident placements survive collection. No
    /// guessed age, capacity, or HD-specific eviction policy is introduced.
    pub fn collect_unused(&mut self) -> usize {
        let before = self.models.len();
        self.models
            .retain(|_path, model| Arc::strong_count(model) > 1);
        before - self.models.len()
    }
}
