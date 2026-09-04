//! Shared lifetime and path lookup for parsed stock BLP texture sources.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{AssetError, AssetPath, AssetStore, BlpTextureSource};

/// Process-local shared cache of immutable compressed BLP sources.
///
/// The owner remains single-threaded and explicit. Parsed sources use [`Arc`]
/// because model bindings, atlas composition, and upload preparation can retain
/// the same source concurrently without decoding all HD mip levels eagerly.
#[derive(Default)]
pub struct BlpTextureCache {
    textures: HashMap<AssetPath, Arc<BlpTextureSource>>,
}

impl BlpTextureCache {
    /// Creates an empty cache without allocating its hash table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of distinct normalized BLP paths retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Reports whether no parsed texture sources are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Returns a shared parsed texture, loading its selected MPQ entry once.
    ///
    /// Higher-priority HD packs replace bytes at the ordinary archive lookup;
    /// their identical virtual path remains the sole cache identity.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when a cache miss cannot resolve or parse the BLP.
    /// Failed loads are not retained.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<Arc<BlpTextureSource>, AssetError> {
        if let Some(texture) = self.textures.get(path) {
            return Ok(Arc::clone(texture));
        }

        let texture = Arc::new(BlpTextureSource::load(store, path)?);
        self.textures.insert(path.clone(), Arc::clone(&texture));
        Ok(texture)
    }

    /// Adopts immutable sources prepared by another single-threaded cache.
    ///
    /// Existing entries retain authority so merging a speculative worker
    /// generation cannot replace a source already selected by this owner.
    pub fn merge(&mut self, prepared: Self) -> usize {
        let before = self.textures.len();
        for (path, texture) in prepared.textures {
            self.textures.entry(path).or_insert(texture);
        }
        self.textures.len() - before
    }

    /// Releases entries held only by the cache and returns the removal count.
    ///
    /// Texture users that still hold an [`Arc`] survive collection. No guessed
    /// age, capacity, or quality-specific eviction policy is introduced.
    pub fn collect_unused(&mut self) -> usize {
        let before = self.textures.len();
        self.textures
            .retain(|_path, texture| Arc::strong_count(texture) > 1);
        before - self.textures.len()
    }
}
