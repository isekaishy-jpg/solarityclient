//! One producer decodes outside metadata locks and publishes exactly one terminal result.

use super::{M2LoadError, M2LoadProducer, M2LoadRequest, Outcome};
use crate::{AssetStore, DecodedM2Model};
use std::sync::Arc;

impl M2LoadProducer {
    /// Admits the producer's consumer before input transfer, publishing refusal to all joiners.
    /// # Errors
    /// Returns the same shared admission error delivered to existing consumers.
    pub fn subscribe_owned(
        self,
        service: solarity_cpu::CpuService,
    ) -> Result<(Self, M2LoadRequest), M2LoadError> {
        match self.subscribe_for(service) {
            Ok(request) => Ok((self, request)),
            Err(error) => {
                let error = M2LoadError::Asset(Arc::new(error));
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    /// Registers another consumer without transferring the producer obligation.
    /// # Errors
    /// Returns metadata admission failure before creating a consumer or producer.
    pub fn subscribe(&self) -> Result<M2LoadRequest, crate::AssetError> {
        self.subscribe_for(solarity_cpu::CpuService::Required)
    }

    /// Registers the producer owner's selected or speculative interest before dispatch.
    /// # Errors
    /// Returns metadata admission failure before creating a consumer or producer.
    pub fn subscribe_for(
        &self,
        service: solarity_cpu::CpuService,
    ) -> Result<M2LoadRequest, crate::AssetError> {
        M2LoadRequest::new(Arc::clone(&self.slot), service)
    }

    /// Admits encoded source inputs before decoding and publishes failures to all joiners.
    /// # Errors
    /// Returns source admission, archive, decode or namespace errors.
    pub fn load_admitted(self, store: &mut AssetStore, budget: &crate::AssetReadBudget) -> Outcome {
        store.with_read_budget(budget, |store| self.load(store))
    }

    /// Decodes the existing primary-profile contract outside all request/cache locks.
    /// # Errors
    /// Returns the original archive/decode error or rejects a mismatched namespace.
    pub fn load(mut self, store: &mut AssetStore) -> Outcome {
        let _profile = solarity_profiling::profile!("asset.m2.shared_decode");
        let decoded = if store.namespace() != self.key.namespace() {
            Err(M2LoadError::Namespace {
                expected: self.key.namespace(),
                actual: store.namespace(),
            })
        } else {
            DecodedM2Model::load_primary_profile_with_class(store, self.key.path(), || {
                match self
                    .slot
                    .demand
                    .strongest()
                    .unwrap_or(self.requested_service)
                {
                    solarity_cpu::CpuService::Speculative => {
                        solarity_cpu::CpuStorageClass::Speculative
                    }
                    _ => solarity_cpu::CpuStorageClass::Required,
                }
            })
            .map(Arc::new)
            .map_err(|error| M2LoadError::Asset(Arc::new(error)))
        };
        // Keep the decoded owner outside the locked admission call, including failure.
        let result =
            match &decoded {
                Ok(model) => {
                    let index =
                        self.service.0.requests.lock().unwrap_or_else(|_| {
                            unreachable!("model request metadata cannot panic")
                        });
                    index
                        .core
                        .as_ref()
                        .unwrap_or_else(|| unreachable!("producer has a registered source cache"))
                        .lock()
                        .insert_shared(self.key.clone(), Arc::clone(model))
                        .map_err(|error| M2LoadError::Asset(Arc::new(error)))
                }
                Err(error) => Err(error.clone()),
            };
        self.publish(result.clone());
        result
    }

    /// Publishes a prerequisite failure to every consumer without attempting a decode.
    pub fn fail(mut self, error: M2LoadError) {
        self.publish(Err(error));
    }

    /// Publication removes pending authority before notifying coordinator waiters.
    fn publish(&mut self, result: Outcome) {
        {
            let mut index = self
                .service
                .0
                .requests
                .lock()
                .unwrap_or_else(|_| unreachable!("model request metadata cannot panic"));
            let removed = index.pending.remove(&self.key);
            assert!(
                removed
                    .as_ref()
                    .is_some_and(|slot| Arc::ptr_eq(slot, &self.slot)),
                "only the registered producer publishes this request"
            );
        }
        *self
            .slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("model result metadata cannot panic")) = Some(result);
        self.finished = true;
        self.slot.ready.notify_all();
        self.slot.publish_dependencies();
    }
}

impl Drop for M2LoadProducer {
    fn drop(&mut self) {
        if !self.finished {
            self.publish(Err(M2LoadError::Abandoned));
        }
    }
}
