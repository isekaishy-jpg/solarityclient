//! Worker-owned archive and texture preparation consume only ready or producer inputs.

use super::super::load_glue_model_generation;
use super::{
    BackdropArchiveOwner, BackdropArchiveState, BackdropModel, BackdropResult, GlueBackdropAssets,
    RuntimeGlueModelError,
};
use solarity_asset::{AssetStore, BlpTextureCache, M2LoadError};
use std::sync::Arc;

impl BackdropArchiveOwner {
    /// Preserves exact precedence and remembers a failed mount instead of retrying it.
    pub(super) fn load(&mut self, model: BackdropModel) -> BackdropResult {
        let state = self.state.get_or_insert_with(|| {
            AssetStore::mount(self.catalog.clone())
                .map(|store| BackdropArchiveState {
                    store,
                    textures: BlpTextureCache::new(),
                })
                .map_err(Arc::new)
        });
        let state = match state {
            Ok(state) => state,
            Err(error) => {
                let error = M2LoadError::Asset(Arc::clone(error));
                if let BackdropModel::Producer(producer) = model {
                    producer.fail(error.clone());
                }
                return Err(Arc::new(RuntimeGlueModelError::SharedModel(error)));
            }
        };
        let model = match model {
            BackdropModel::Ready(model) => model,
            BackdropModel::Producer(producer) => producer
                .load(&mut state.store)
                .map_err(|error| Arc::new(RuntimeGlueModelError::SharedModel(error)))?,
        };
        let (model, textures) =
            load_glue_model_generation(model, &mut state.textures, &mut state.store)
                .map_err(Arc::new)?;
        Ok(Arc::new(GlueBackdropAssets { model, textures }))
    }
}
