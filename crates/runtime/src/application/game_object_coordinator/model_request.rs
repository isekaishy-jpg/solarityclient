//! Main-owned source waits keep dependent GameObject jobs outside the CPU queue.

use super::{
    GameObjectM2Input, ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectPresentation,
    RuntimeGameObjectResourceKind,
};
use solarity_asset::{AssetResourceKey, M2Load, M2LoadError};
use std::sync::Arc;

impl RuntimeGameObjectPresentation {
    /// Existing source demand must register even when the CPU queue has no free task slot.
    pub(super) fn observe_model_request(
        &mut self,
        request: &ResourceRequest,
    ) -> Result<(), RuntimeGameObjectError> {
        if self
            .model_wait
            .as_ref()
            .is_some_and(|(key, _)| key != request)
        {
            self.model_wait = None;
        }
        if request.kind != RuntimeGameObjectResourceKind::M2 || self.model_wait.is_some() {
            return Ok(());
        }
        let Some(catalog) = self.worker_catalog.as_ref() else {
            return Ok(());
        };
        let key = AssetResourceKey::new(catalog.namespace(), request.path.clone());
        if let Some(waiting) = catalog
            .model_cache_service()
            .join_pending(&key, solarity_cpu::CpuService::Required)
            .map_err(|error| M2LoadError::Asset(Arc::new(error)))?
        {
            self.model_wait = Some((request.clone(), waiting));
        }
        Ok(())
    }

    /// A replaced request releases only this consumer; an existing producer keeps its other owners.
    pub(super) fn request_model(
        &mut self,
        request: &ResourceRequest,
    ) -> Result<Option<GameObjectM2Input>, RuntimeGameObjectError> {
        if self
            .model_wait
            .as_ref()
            .is_some_and(|(key, _)| key != request)
        {
            self.model_wait = None;
        }
        if let Some((_, waiting)) = &self.model_wait {
            let Some(result) = waiting.poll() else {
                return Ok(None);
            };
            self.model_wait = None;
            return result
                .map(|model| Some(GameObjectM2Input::Ready(model)))
                .map_err(Into::into);
        }
        let catalog = self
            .worker_catalog
            .as_ref()
            .ok_or(RuntimeGameObjectError::MissingWorkerCatalog)?;
        let key = AssetResourceKey::new(catalog.namespace(), request.path.clone());
        match catalog
            .model_cache_service()
            .request(&key)
            .map_err(|error| M2LoadError::Asset(Arc::new(error)))?
        {
            M2Load::Ready(model) => Ok(Some(GameObjectM2Input::Ready(model))),
            M2Load::Producer(producer) => Ok(Some(GameObjectM2Input::Producer(producer))),
            M2Load::Pending(waiting) => {
                self.model_wait = Some((request.clone(), waiting));
                Ok(None)
            }
        }
    }
}
