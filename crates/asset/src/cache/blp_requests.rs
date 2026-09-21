//! Namespace texture requests publish once and suspend consumers through typed readiness.
use super::source_dependency::{SourceDependency, SourceSlot};
use super::source_storage::{ControlOwner, SourceStorage};
use crate::{
    AssetError, AssetNamespaceId, AssetReadBudget, AssetResourceKey, AssetStore, BlpTextureSource,
};
use solarity_cpu::{
    CpuError, CpuService, CpuServiceControl, CpuServiceInterest, CpuStorageBudget, CpuStorageClass,
};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Shared source failures are distinct from scheduler cancellation and abandonment.
#[derive(Clone, Debug, Error)]
pub enum BlpLoadError {
    /// CPU metadata or dependency admission failed.
    #[error(transparent)]
    Cpu(Arc<CpuError>),
    /// Every joined consumer observes the original archive/decoder failure.
    #[error(transparent)]
    Asset(Arc<AssetError>),
    /// A producer cannot read a different immutable archive selection.
    #[error("texture request namespace {expected:?} differs from reader {actual:?}")]
    Namespace {
        /// Selection captured by the request.
        expected: AssetNamespaceId,
        /// Selection supplied by the reader.
        actual: AssetNamespaceId,
    },
    /// The unique producer ended before publishing.
    #[error("texture producer ended before publication")]
    Abandoned,
}
impl From<AssetError> for BlpLoadError {
    fn from(error: AssetError) -> Self {
        Self::Asset(Arc::new(error))
    }
}
impl From<CpuError> for BlpLoadError {
    fn from(error: CpuError) -> Self {
        Self::Cpu(Arc::new(error))
    }
}
type Outcome = Result<BlpTextureSource, BlpLoadError>;
type Slot = SourceSlot<BlpTextureSource, BlpLoadError>;
/// Each consumer owns a typed outcome and its own admitted readiness/demand edge.
pub type BlpLoadDependency = SourceDependency<BlpTextureSource, BlpLoadError>;

struct State {
    ready: crate::AssetStorageMap<AssetResourceKey, crate::texture::BlpTextureWeak>,
    pending: crate::AssetStorageMap<AssetResourceKey, Arc<Slot>>,
    control: ControlOwner,
}
/// Cloned catalogs share authority without sharing mutable archive handles.
/// Ready entries are weak: cache/consumer owners retain the actual payload charge.
#[derive(Clone)]
pub struct BlpCacheService(Arc<Mutex<State>>);
impl Default for BlpCacheService {
    fn default() -> Self {
        Self::with_storage(Arc::default())
    }
}
impl std::fmt::Debug for BlpCacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlpCacheService").finish_non_exhaustive()
    }
}
/// A ready immutable payload, an existing producer, or the sole decode obligation.
pub enum BlpLoad {
    /// Shares the published payload without reading or parsing again.
    Ready(BlpTextureSource),
    /// Join by polling or declaring a CPU readiness dependency.
    Pending(BlpLoadRequest),
    /// Perform and publish one source decode on the caller's admitted worker.
    Producer(BlpLoadProducer),
}
/// Each consumer has independent demand; dropping one never cancels another.
#[derive(Clone)]
pub struct BlpLoadRequest {
    slot: Arc<Slot>,
    interest: CpuServiceInterest,
}
/// Dropping the sole producer publishes abandonment to every joined consumer.
pub struct BlpLoadProducer {
    service: BlpCacheService,
    key: AssetResourceKey,
    slot: Arc<Slot>,
    finished: bool,
}
impl BlpCacheService {
    pub(crate) fn with_storage(storage: Arc<SourceStorage>) -> Self {
        Self(Arc::new(Mutex::new(State {
            ready: crate::AssetStorageMap::metadata(),
            pending: crate::AssetStorageMap::metadata(),
            control: ControlOwner::for_arc::<Mutex<State>>(&storage),
        })))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|_| unreachable!("texture request metadata cannot panic"))
    }
    /// Claims or joins exact namespace/path identity before archive access.
    /// # Errors
    /// Returns metadata admission failure before creating a consumer or producer.
    pub fn request_for(
        &self,
        key: &AssetResourceKey,
        service: CpuService,
    ) -> Result<BlpLoad, AssetError> {
        let mut state = self.lock();
        if let Some(source) = state
            .ready
            .get(key)
            .and_then(crate::texture::BlpTextureWeak::upgrade)
        {
            return Ok(BlpLoad::Ready(source));
        }
        state.ready.remove(key);
        if let Some(slot) = state.pending.get(key) {
            return Ok(BlpLoad::Pending(BlpLoadRequest::new(
                Arc::clone(slot),
                service,
            )?));
        }
        let storage = state.control.budget().cloned();
        let budget = storage
            .clone()
            .map(|storage| AssetReadBudget::for_service(storage, CpuService::Required));
        let capacity = state.pending.len() + 1;
        let slot = Slot::new(storage.as_ref())?;
        state.pending.reserve(budget.as_ref(), capacity)?;
        state
            .pending
            .insert(budget.as_ref(), key.clone(), Arc::clone(&slot))?;
        Ok(BlpLoad::Producer(BlpLoadProducer {
            service: self.clone(),
            key: key.clone(),
            slot,
            finished: false,
        }))
    }
    pub(super) fn ready(&self, key: &AssetResourceKey) -> Option<BlpTextureSource> {
        self.lock()
            .ready
            .get(key)
            .and_then(crate::texture::BlpTextureWeak::upgrade)
    }
    /// Synchronous cache owners also reuse the namespace's first published payload.
    pub(super) fn publish_ready(
        &self,
        key: AssetResourceKey,
        source: BlpTextureSource,
    ) -> Result<BlpTextureSource, AssetError> {
        let mut state = self.lock();
        if let Some(existing) = state
            .ready
            .get(&key)
            .and_then(crate::texture::BlpTextureWeak::upgrade)
        {
            return Ok(existing);
        }
        let budget = state
            .control
            .budget()
            .cloned()
            .map(|storage| AssetReadBudget::for_service(storage, CpuService::Required));
        state
            .ready
            .insert(budget.as_ref(), key, source.downgrade())?;
        Ok(source)
    }
    /// Drops only expired weak index metadata; live payloads and pending work are untouched.
    pub fn collect_unused(&self) {
        self.lock().ready.retain(|_, source| source.is_alive());
    }
}
impl BlpLoadRequest {
    fn new(slot: Arc<Slot>, service: CpuService) -> Result<Self, AssetError> {
        let interest = slot.subscribe(service)?;
        Ok(Self { slot, interest })
    }
    /// Binds the unique producer's admitted scheduling identity once.
    #[must_use]
    pub fn bind_service(&self, control: CpuServiceControl) -> bool {
        self.slot.demand.bind(control)
    }
    /// Updates only this consumer's demand; other consumers retain their own interests.
    pub fn set_service(&self, service: CpuService) {
        self.interest.set_service(service);
    }
    /// Reads durable publication without parking a CPU worker.
    #[must_use]
    pub fn poll(&self) -> Option<Outcome> {
        self.slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("texture result metadata cannot panic"))
            .clone()
    }
    /// Admits one consumer's typed edge before releasing its worker.
    /// # Errors
    /// Returns CPU metadata pressure without losing the request.
    pub fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<BlpLoadDependency, CpuError> {
        self.slot
            .dependency(budget, class, self.interest.clone(), || {
                BlpLoadError::Abandoned
            })
    }
}
impl BlpLoadProducer {
    /// Admits the producer's consumer before input transfer, publishing refusal to all joiners.
    /// # Errors
    /// Returns the same shared admission error delivered to existing consumers.
    pub fn subscribe_owned(
        self,
        service: CpuService,
    ) -> Result<(Self, BlpLoadRequest), BlpLoadError> {
        match self.subscribe_for(service) {
            Ok(request) => Ok((self, request)),
            Err(error) => {
                let error = BlpLoadError::Asset(Arc::new(error));
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    /// Captures the producer owner's independent interest before worker admission.
    /// # Errors
    /// Returns metadata admission failure before creating a consumer or producer.
    pub fn subscribe_for(&self, service: CpuService) -> Result<BlpLoadRequest, AssetError> {
        BlpLoadRequest::new(Arc::clone(&self.slot), service)
    }
    /// Decodes outside all metadata locks and publishes the original result once.
    /// # Errors
    /// Preserves admission, namespace, archive and decoder failures for every joiner.
    pub fn load(mut self, store: &mut AssetStore, budget: &AssetReadBudget) -> Outcome {
        let result = if store.namespace() != self.key.namespace() {
            Err(BlpLoadError::Namespace {
                expected: self.key.namespace(),
                actual: store.namespace(),
            })
        } else {
            store
                .with_read_budget(budget, |store| {
                    BlpTextureSource::load(store, self.key.path())
                })
                .and_then(|source| self.service.publish_ready(self.key.clone(), source))
                .and_then(|source| {
                    source.admit(budget)?;
                    Ok(source)
                })
                .map_err(BlpLoadError::from)
        };
        self.publish(result.clone());
        result
    }
    /// Publishes a prerequisite error without decoding.
    pub fn fail(mut self, error: BlpLoadError) {
        self.publish(Err(error));
    }
    fn publish(&mut self, result: Outcome) {
        let removed = self.service.lock().pending.remove(&self.key);
        assert!(
            removed
                .as_ref()
                .is_some_and(|slot| Arc::ptr_eq(slot, &self.slot)),
            "only the registered texture producer publishes"
        );
        *self
            .slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("texture result metadata cannot panic")) =
            Some(result);
        self.finished = true;
        self.slot.ready.notify_all();
        self.slot.publish_dependencies();
    }
}
impl Drop for BlpLoadProducer {
    fn drop(&mut self) {
        if !self.finished {
            self.publish(Err(BlpLoadError::Abandoned));
        }
    }
}
