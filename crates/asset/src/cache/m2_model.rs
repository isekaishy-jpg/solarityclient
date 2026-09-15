//! Shared lifetime and path lookup for decoded build-12340 M2 models.

use super::resource::{ResourceCache, ResourceLease};

use crate::model::canonical_model_path;
use crate::{AssetError, AssetPath, AssetResourceKey, AssetStore, DecodedM2Model};

/// Process-local shared cache of immutable decoded M2 and SKIN data.
///
/// The owner remains single-threaded and explicit. Individual model values use
/// [`ResourceLease`] because scene instances and render preparation legitimately retain
/// the same large immutable model concurrently, including HD replacements.
#[derive(Default)]
pub struct M2ModelCache {
    models: ResourceCache<AssetResourceKey, DecodedM2Model>,
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

    /// Returns the number of model sources retained across archive namespaces.
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
    /// The cache key includes the immutable archive namespace and normalized
    /// virtual path. Stores from the same catalog select identical content.
    /// A higher-priority HD
    /// pack therefore selects larger bytes at the ordinary load boundary and
    /// does not create another quality-specific cache namespace.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when a cache miss cannot resolve or decode the M2
    /// and its stock-selected primary SKIN profile. Failed loads are not retained.
    pub fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<ResourceLease<DecodedM2Model>, AssetError> {
        let canonical_path = Self::canonical_path(path)?;
        let key = AssetResourceKey::new(store.namespace(), canonical_path.clone());
        if let Some(model) = self.models.get(&key) {
            return Ok(model);
        }

        let model = DecodedM2Model::load_primary_profile(store, &canonical_path)?;
        self.models.insert(key, model)
    }

    /// Visits final-release notifications and returns the number of collected entries.
    ///
    /// This preserves the existing explicit collection boundary. Live scene
    /// leases retain the same immutable payload; only release metadata is locked.
    /// Stock-qualified timed retention remains a separate policy contract.
    pub fn collect_unused(&mut self) -> usize {
        self.models.collect_unused()
    }
}
