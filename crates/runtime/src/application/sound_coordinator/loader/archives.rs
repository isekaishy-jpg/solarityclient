//! Worker-private archive ownership for shared selected sound requests.

use super::super::RuntimeSoundError;
use solarity_asset::{ArchiveCatalog, AssetStore};
use solarity_media::{EncodedSound, SoundCache, SoundLoadRequest};
use std::sync::Arc;

/// A single archive owner, lazily mounted once on a CPU worker.
pub(super) struct SoundReadAssets {
    pub(super) catalog: ArchiveCatalog,
    pub(super) store: Option<Result<AssetStore, String>>,
}

impl SoundReadAssets {
    /// Preserves the discovered precedence and never retries a failed mount.
    pub(super) fn read(
        &mut self,
        request: &SoundLoadRequest,
        budget: &solarity_asset::AssetReadBudget,
    ) -> Result<Arc<EncodedSound>, RuntimeSoundError> {
        let store = self.store.get_or_insert_with(|| {
            AssetStore::mount(self.catalog.clone()).map_err(|error| error.to_string())
        });
        let store = store
            .as_mut()
            .map_err(|message| RuntimeSoundError::LoaderUnavailable {
                message: message.clone(),
            })?;
        store
            .with_read_budget(budget, |store| {
                SoundCache::new().load(store, request.path())
            })
            .map_err(|error| RuntimeSoundError::Engine(error.into()))
    }
}
