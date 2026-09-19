//! Owned character preparation state and finite selected-appearance service steps.

mod glue;
mod owner;

use super::{GlueCharacterWorkerFailure, ResidentGlueCharacterModel, RuntimePlayerError};
pub(super) use glue::glue_steps;
pub(super) use owner::with_worker_presentation;
use solarity_asset::{AssetStore, BlpTextureCache, M2ModelCache};

/// The coordinator transfers this entire bank to one task after reserving admission.
#[derive(Default)]
pub(super) struct GlueCharacterWorkerCache {
    store: Option<AssetStore>,
    models: M2ModelCache,
    textures: BlpTextureCache,
}

impl GlueCharacterWorkerCache {
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

/// Coalesced Glue completion returns cache ownership independently of presentation success.
pub(super) struct GlueWorkerCompletion {
    pub(super) cache: GlueCharacterWorkerCache,
    pub(super) result: Result<Option<ResidentGlueCharacterModel>, GlueCharacterWorkerFailure>,
}

/// Population publication likewise restores its cache before handling an obsolete or failed result.
pub(super) struct PopulationWorkerCompletion<T> {
    pub(super) cache: GlueCharacterWorkerCache,
    pub(super) result: Result<T, RuntimePlayerError>,
}
