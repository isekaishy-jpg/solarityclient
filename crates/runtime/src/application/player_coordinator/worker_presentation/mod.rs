//! Owned character preparation state and finite selected-appearance service steps.

mod glue;
mod model_request;
mod owner;

use super::RuntimePlayerError;
pub(super) use glue::prepare_glue_character;
pub(super) use model_request::{AppearanceRequest, AppearanceTask};
pub(super) use owner::with_worker_presentation;
use solarity_asset::{AssetStore, BlpTextureCache, M2ModelCache};

/// The coordinator transfers this entire bank to one task after reserving admission.
#[derive(Default)]
pub(super) struct AppearanceWorkerCache {
    store: Option<AssetStore>,
    models: M2ModelCache,
    textures: BlpTextureCache,
}

impl AppearanceWorkerCache {
    /// Mounts before transferring cache fields so a failed source preserves the bank.
    pub(in crate::application::player_coordinator) fn mount(
        &mut self,
        catalog: &solarity_asset::ArchiveCatalog,
    ) -> Result<(), solarity_asset::AssetError> {
        if self.store.is_none() {
            self.store = Some(AssetStore::mount(catalog.clone())?);
        }
        Ok(())
    }
}

/// Appearance publication restores its cache before handling an obsolete or failed result.
pub(super) struct AppearanceCompletion<T> {
    pub(super) cache: AppearanceWorkerCache,
    pub(super) result: Result<T, RuntimePlayerError>,
}
