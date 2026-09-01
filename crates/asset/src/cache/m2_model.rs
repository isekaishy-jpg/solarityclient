//! Shared lifetime and path lookup for decoded build-12340 M2 models.

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::canonical_model_path;
use crate::{AssetError, AssetPath, AssetStore, DecodedM2Model};

/// Process-local shared cache of immutable decoded M2 and SKIN data.
///
/// The owner remains single-threaded and explicit. Individual model values use
/// [`Arc`] because scene instances and render preparation legitimately retain
/// the same large immutable model concurrently, including HD replacements.
#[derive(Default)]
pub struct M2ModelCache {
    models: HashMap<AssetPath, Arc<DecodedM2Model>>,
}

impl M2ModelCache {
    /// Creates an empty cache without allocating its hash table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the stock cache key for an M2, MDL, or MDX model path.
    ///
    /// DBC tables retain legacy extensions, while decoded models expose the
    /// canonical `.M2` archive path. Residency owners use this boundary so
    /// those two representations do not appear to be different models.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the path has no build-12340 model extension.
    pub fn canonical_path(path: &AssetPath) -> Result<AssetPath, AssetError> {
        canonical_model_path(path)
    }

    /// Returns the number of distinct normalized M2 paths retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Reports whether no decoded models are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Returns a shared decoded model, loading its selected MPQ entries once.
    ///
    /// The cache key is only the normalized virtual path because an
    /// [`AssetStore`] mounts one immutable archive stack. A higher-priority HD
    /// pack therefore selects larger bytes at the ordinary load boundary and
    /// does not create another cache namespace.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when a cache miss cannot resolve or decode the M2
    /// and its stock-selected primary SKIN profile. Failed loads are not retained.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<Arc<DecodedM2Model>, AssetError> {
        let canonical_path = Self::canonical_path(path)?;
        if let Some(model) = self.models.get(&canonical_path) {
            return Ok(Arc::clone(model));
        }

        let model = Arc::new(DecodedM2Model::load_primary_profile(
            store,
            &canonical_path,
        )?);
        self.models.insert(canonical_path, Arc::clone(&model));
        Ok(model)
    }

    /// Releases entries held only by the cache and returns the removal count.
    ///
    /// This mirrors stock's explicit M2 cache garbage-collection boundary.
    /// Models retained by a scene instance survive without locks or a guessed
    /// age/capacity eviction policy.
    pub fn collect_unused(&mut self) -> usize {
        let before = self.models.len();
        self.models
            .retain(|_path, model| Arc::strong_count(model) > 1);
        before - self.models.len()
    }
}
