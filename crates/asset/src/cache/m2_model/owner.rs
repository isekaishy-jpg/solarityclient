//! Decode stays outside cache locks; source generations survive qualified release.

use super::super::resource::{ResourceCache, ResourceCacheClock, ResourceLease};
use super::{M2ModelCache, ModelCacheCore};
use crate::model::canonical_model_path;
use crate::{AssetError, AssetPath, AssetResourceKey, AssetStore, DecodedM2Model};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

impl ModelCacheCore {
    /// This lock protects index ownership only, never archive I/O, decoding or final disposal.
    pub(super) fn lock(&self) -> MutexGuard<'_, ResourceCache<AssetResourceKey, DecodedM2Model>> {
        self.models
            .lock()
            .unwrap_or_else(|_| unreachable!("model cache metadata cannot panic"))
    }
}

impl Default for M2ModelCache {
    fn default() -> Self {
        Self::new()
    }
}

impl M2ModelCache {
    /// Creates qualified source retention using the process's monotonic cache clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(ResourceCacheClock::monotonic())
    }

    /// Selects explicit cache time without using animation or frame-count age.
    #[must_use]
    pub fn with_clock(clock: ResourceCacheClock) -> Self {
        Self {
            core: Arc::new(ModelCacheCore {
                models: Mutex::new(ResourceCache::with_retention(clock)),
                owned: AtomicBool::new(true),
            }),
            namespaces: HashSet::new(),
        }
    }

    /// Returns the stock-normalized M2/MDL/MDX path without reading archive data.
    /// # Errors
    /// Rejects paths without a supported model extension.
    pub fn canonical_path(path: &AssetPath) -> Result<AssetPath, AssetError> {
        canonical_model_path(path)
    }

    /// Number of immutable generations still retained, including qualified grace periods.
    #[must_use]
    pub fn len(&self) -> usize {
        self.core.lock().len()
    }

    /// Reports whether no source remains in this cache.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.core.lock().is_empty()
    }

    /// Returns the exact namespace/path generation and its selected primary SKIN.
    /// Missing inputs retain their existing errors; a cache hit creates no scene membership.
    /// # Errors
    /// Reports archive/decode errors or exhausted release-generation identity.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<ResourceLease<DecodedM2Model>, AssetError> {
        let canonical_path = Self::canonical_path(path)?;
        let key = AssetResourceKey::new(store.namespace(), canonical_path.clone());
        if let Some(model) = store.model_cache_service().ready(&key)? {
            return Ok(model);
        }
        if self.namespaces.insert(store.namespace()) {
            store.model_cache_service().register(&self.core);
        }
        if let Some(model) = self.core.lock().get(&key)? {
            return Ok(model);
        }
        // This owner is the only decoder for its index. The maintenance handle
        // can detach expired values, but never performs a competing load.
        let model = Arc::new(DecodedM2Model::load_primary_profile(
            store,
            &canonical_path,
        )?);
        let result = self.core.lock().insert_shared(key, Arc::clone(&model));
        // This outer pin also covers admission failure, not only successful insertion.
        drop(model);
        result
    }

    /// Collects the expired released prefix using stock's signed 10,000 ms test.
    /// Ordinary runtime cleanup uses the namespace service on CPU workers instead.
    /// There is no generic pressure/force bypass and live consumers remain pinned.
    pub fn collect_unused(&mut self) -> usize {
        let now = self.core.lock().collection_time();
        let mut count = 0;
        loop {
            let retired = self.core.lock().take_unused(now);
            let Some(retired) = retired else {
                return count;
            };
            drop(retired);
            count += 1;
        }
    }
}

impl Drop for M2ModelCache {
    fn drop(&mut self) {
        self.core.owned.store(false, Ordering::Release);
        self.core.lock().mark_changed();
    }
}
