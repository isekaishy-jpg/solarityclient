//! Shared primary sources gate appearance work without occupying a waiting worker.

use super::super::super::{
    RuntimePlayerError, RuntimePlayerPresentation, RuntimePlayerSharedCatalogs,
};
use super::super::{AppearanceCompletion, AppearanceWorkerCache, with_worker_presentation};
use solarity_asset::{ArchiveCatalog, DecodedM2Model, M2LoadProducer, ResourceLease};
use solarity_rendering::CharacterComponentTextureLevel;

/// Main selects immutable appearance inputs; the worker owns the preparation call.
pub(super) type Prepare<T> = Box<
    dyn FnOnce(
            &mut RuntimePlayerPresentation,
            ResourceLease<DecodedM2Model>,
        ) -> Result<T, RuntimePlayerError>
        + Send,
>;

/// One exclusive archive/derived-cache bank returns independently of source success.
pub(super) struct AppearanceBank {
    pub(super) catalog: ArchiveCatalog,
    pub(super) catalogs: RuntimePlayerSharedCatalogs,
    pub(super) level: CharacterComponentTextureLevel,
    pub(super) cache: AppearanceWorkerCache,
}

/// A reserved producer owns decoding; a ready lease avoids any second model lookup.
pub(super) enum ModelInput {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
}

impl AppearanceBank {
    /// Runs archive work outside request metadata locks and restores the bank on error.
    pub(super) fn prepare<T>(
        mut self,
        model: ModelInput,
        prepare: Prepare<T>,
    ) -> AppearanceCompletion<T> {
        // A producer must publish the actual mount error to its joiners, not
        // merely disappear and turn it into an abandoned-producer diagnosis.
        if let Err(error) = self.cache.mount(&self.catalog) {
            let error = match model {
                ModelInput::Producer(producer) => {
                    let error = solarity_asset::M2LoadError::Asset(std::sync::Arc::new(error));
                    producer.fail(error.clone());
                    RuntimePlayerError::from(error)
                }
                ModelInput::Ready(_) => RuntimePlayerError::from(error),
            };
            return self.failed(error);
        }
        let result = with_worker_presentation(
            self.catalog,
            self.catalogs,
            self.level,
            &mut self.cache,
            |presentation| {
                let model = match model {
                    ModelInput::Ready(model) => model,
                    ModelInput::Producer(producer) => {
                        producer.load(&mut presentation.assets.borrow_mut())?
                    }
                };
                prepare(presentation, model)
            },
        );
        AppearanceCompletion {
            cache: self.cache,
            result,
        }
    }

    /// A cancelled or failed prerequisite leaves all worker-local caches owned.
    pub(super) fn failed<T>(self, error: RuntimePlayerError) -> AppearanceCompletion<T> {
        AppearanceCompletion {
            cache: self.cache,
            result: Err(error),
        }
    }
}
