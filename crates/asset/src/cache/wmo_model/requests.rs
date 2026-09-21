//! Namespace-wide root/group requests share one producer without parking CPU workers.

use super::super::resource::{ResourceCache, ResourceLease};
use super::super::source_dependency::{SourceDependency, SourceSlot};
use super::super::source_storage::{ControlOwner, RetirementSignal, SourceStorage};
use crate::{AssetError, AssetNamespaceId, AssetResourceKey, AssetStore, DecodedWorldModel};
use solarity_cpu::{
    CpuError, CpuService, CpuServiceControl, CpuServiceInterest, CpuStorageBudget, CpuStorageClass,
};
use std::sync::{Arc, Mutex, atomic::Ordering};
use thiserror::Error;

/// Source failures remain separate from scheduler cancellation and producer abandonment.
#[derive(Clone, Debug, Error)]
pub enum WmoLoadError {
    /// Producer admission failed before source work could proceed.
    #[error(transparent)]
    Cpu(Arc<CpuError>),
    /// Joined consumers retain the same original archive or decoder failure.
    #[error(transparent)]
    Asset(Arc<AssetError>),
    /// The mounted reader must belong to the captured archive selection.
    #[error("world model request namespace {expected:?} differs from reader {actual:?}")]
    Namespace {
        /// Namespace captured at request admission.
        expected: AssetNamespaceId,
        /// Namespace owned by the supplied archive reader.
        actual: AssetNamespaceId,
    },
    /// The sole producer was dropped before source publication.
    #[error("world model producer ended before publication")]
    Abandoned,
}

type Outcome = Result<ResourceLease<DecodedWorldModel>, WmoLoadError>;
type Slot = SourceSlot<ResourceLease<DecodedWorldModel>, WmoLoadError>;

/// A consumer owns its own budgeted dependency edge and live priority contribution.
pub type WmoLoadDependency = SourceDependency<ResourceLease<DecodedWorldModel>, WmoLoadError>;

/// Pending authority and ready residency have distinct lifetimes and locks.
struct State {
    models: Mutex<ResourceCache<AssetResourceKey, DecodedWorldModel>>,
    pending: Mutex<crate::AssetStorageMap<AssetResourceKey, Arc<Slot>>>,
    changed: RetirementSignal,
    control: ControlOwner,
}

/// Catalog clones share identity; rediscovery creates another service and archive namespace.
#[derive(Clone)]
pub struct WmoCacheService(Arc<State>);

impl Default for WmoCacheService {
    fn default() -> Self {
        Self::with_storage(Arc::default())
    }
}
impl WmoCacheService {
    pub(crate) fn with_storage(storage: Arc<SourceStorage>) -> Self {
        let changed = RetirementSignal::new(&storage);
        let models = ResourceCache::default();
        Self(Arc::new(State {
            models: Mutex::new(models),
            pending: Mutex::new(crate::AssetStorageMap::metadata()),
            control: ControlOwner::for_arc::<State>(&storage),
            changed,
        }))
    }
}
impl std::fmt::Debug for WmoCacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WmoCacheService").finish_non_exhaustive()
    }
}

/// A ready root/group lease, an existing producer, or the unique new decode obligation.
pub enum WmoLoad {
    /// An already admitted root and all its numbered groups.
    Ready(ResourceLease<DecodedWorldModel>),
    /// Another owner must publish; this consumer declares a dependency.
    Pending(WmoLoadRequest),
    /// The caller owns unique decode and terminal publication.
    Producer(WmoLoadProducer),
}

/// Each consumer can withdraw without cancelling another consumer's source demand.
#[derive(Clone)]
pub struct WmoLoadRequest {
    slot: Arc<Slot>,
    interest: CpuServiceInterest,
}

/// Unique publication ownership reports abandoned work rather than stranding a dependent service.
pub struct WmoLoadProducer {
    service: WmoCacheService,
    key: AssetResourceKey,
    slot: Arc<Slot>,
    finished: bool,
    preparation: Option<Box<crate::world_model::map_obj_read::WorldModelPreparation>>,
}

impl WmoCacheService {
    /// Join exact namespace/root identity before archive work. Locks contain no decode or disposal.
    /// # Errors
    /// Returns exhausted source identity when ready-cache lease publication is refused.
    pub fn request_for(
        &self,
        key: &AssetResourceKey,
        service: CpuService,
    ) -> Result<WmoLoad, AssetError> {
        let mut pending = self
            .0
            .pending
            .lock()
            .unwrap_or_else(|_| unreachable!("WMO request metadata cannot panic"));
        let ready = {
            let mut models = self
                .0
                .models
                .lock()
                .unwrap_or_else(|_| unreachable!("WMO cache metadata cannot panic"));
            models.admit(self.0.control.budget())?;
            models.subscribe(&self.0.changed)?;
            models.get(key)?
        };
        if let Some(model) = ready {
            return Ok(WmoLoad::Ready(model));
        }
        if let Some(slot) = pending.get(key) {
            return Ok(WmoLoad::Pending(WmoLoadRequest::new(
                Arc::clone(slot),
                service,
            )?));
        }
        let budget = self
            .0
            .control
            .budget()
            .cloned()
            .map(|storage| crate::AssetReadBudget::for_service(storage, CpuService::Required));
        let capacity = pending.len() + 1;
        let slot = Slot::new(self.0.control.budget())?;
        pending.reserve(budget.as_ref(), capacity)?;
        pending.insert(budget.as_ref(), key.clone(), Arc::clone(&slot))?;
        Ok(WmoLoad::Producer(WmoLoadProducer {
            service: self.clone(),
            key: key.clone(),
            slot,
            finished: false,
            preparation: None,
        }))
    }

    /// Final external release coalesces one durable maintenance notice.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.0.changed.swap(false, Ordering::AcqRel)
    }

    /// Read only the release-list head; publication and reacquisition may also set the change bit.
    #[must_use]
    pub fn has_pending_retirement(&self) -> bool {
        self.0
            .models
            .lock()
            .unwrap_or_else(|_| unreachable!("WMO cache metadata cannot panic"))
            .next_delay_ms()
            .is_some()
    }

    /// WMO collection keeps the existing immediate-unused policy; no M2 grace period is guessed.
    /// This must execute on an admitted retirement worker, outside any source/table lock.
    pub fn collect_step(&self) -> std::ops::ControlFlow<()> {
        for _ in 0..16 {
            let retired = {
                let mut models = self
                    .0
                    .models
                    .lock()
                    .unwrap_or_else(|_| unreachable!("WMO cache metadata cannot panic"));
                let now = models.collection_time();
                models.take_unused(now)
            };
            let Some(retired) = retired else {
                return std::ops::ControlFlow::Break(());
            };
            drop(retired);
        }
        std::ops::ControlFlow::Continue(())
    }
}

impl WmoLoadRequest {
    /// Register this consumer's independent live scheduling interest.
    fn new(slot: Arc<Slot>, service: CpuService) -> Result<Self, AssetError> {
        let interest = slot.subscribe(service)?;
        Ok(Self { slot, interest })
    }
    /// Connects the original admitted producer's scheduling identity exactly once.
    #[must_use]
    pub fn bind_service(&self, control: CpuServiceControl) -> bool {
        self.slot.demand.bind(control)
    }
    /// Changes only this consumer's live demand.
    pub fn set_service(&self, service: CpuService) {
        self.interest.set_service(service);
    }
    /// Durable source completion is nonblocking and preserves the original error owner.
    #[must_use]
    pub fn poll(&self) -> Option<Outcome> {
        self.slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("WMO result metadata cannot panic"))
            .clone()
    }
    /// Reserves the domain edge before an owned loader suspends.
    /// # Errors
    /// Returns ordinary CPU metadata/slot admission refusal without losing the request.
    pub fn dependency(
        &self,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<WmoLoadDependency, CpuError> {
        self.slot
            .dependency(budget, class, self.interest.clone(), || {
                WmoLoadError::Abandoned
            })
    }
}

impl WmoLoadProducer {
    /// Admits the producer's consumer before input transfer, publishing refusal to all joiners.
    /// # Errors
    /// Returns the same shared admission error delivered to existing consumers.
    pub fn subscribe_owned(
        self,
        service: CpuService,
    ) -> Result<(Self, WmoLoadRequest), WmoLoadError> {
        match self.subscribe_for(service) {
            Ok(request) => Ok((self, request)),
            Err(error) => {
                let error = WmoLoadError::Asset(Arc::new(error));
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    /// Pins and binds the producer owner's interest before decoding starts.
    /// # Errors
    /// Returns metadata admission failure before creating a consumer or producer.
    pub fn subscribe_for(&self, service: CpuService) -> Result<WmoLoadRequest, AssetError> {
        WmoLoadRequest::new(Arc::clone(&self.slot), service)
    }
    /// Drives the finite source steps for callers without an executor continuation.
    /// # Errors
    /// Preserves original archive/decoder failures and rejects a different namespace.
    pub fn load(mut self, store: &mut AssetStore) -> Outcome {
        loop {
            if let Some(model) = self.step(store)? {
                return Ok(model);
            }
        }
    }

    /// Advances one root, one group, or final validation; only a complete source is published.
    /// The caller retains this producer across CPU service yields. Dropping it wakes joiners.
    /// # Errors
    /// Publishes the original source failure to every consumer, or rejects a different namespace.
    /// # Panics
    /// Panics if advanced after terminal publication.
    pub fn step(
        &mut self,
        store: &mut AssetStore,
    ) -> Result<Option<ResourceLease<DecodedWorldModel>>, WmoLoadError> {
        assert!(
            !self.finished,
            "WMO producer cannot advance after publication"
        );
        let _profile = solarity_profiling::profile!("asset.wmo.shared_decode_step");
        let decoded = self.decode_step(store);
        let result = match decoded {
            Ok(None) => return Ok(None),
            Ok(Some(model)) => {
                // The outer pin drops after the metadata lock, including insertion failure.
                let model = Arc::new(model);
                self.service
                    .0
                    .models
                    .lock()
                    .unwrap_or_else(|_| unreachable!("WMO cache metadata cannot panic"))
                    .insert_shared(self.key.clone(), Arc::clone(&model))
                    .map_err(|e| WmoLoadError::Asset(Arc::new(e)))
            }
            Err(error) => Err(error),
        };
        self.publish(result.clone());
        result.map(Some)
    }

    /// Individual archive decompression remains an indivisible codec call; group boundaries yield.
    fn decode_step(
        &mut self,
        store: &mut AssetStore,
    ) -> Result<Option<DecodedWorldModel>, WmoLoadError> {
        if store.namespace() != self.key.namespace() {
            return Err(WmoLoadError::Namespace {
                expected: self.key.namespace(),
                actual: store.namespace(),
            });
        }
        let result = (|| {
            let Some(preparation) = &mut self.preparation else {
                self.preparation = Some(Box::new(
                    crate::world_model::map_obj_read::WorldModelPreparation::begin(
                        store,
                        self.key.path(),
                    )?,
                ));
                return Ok(None);
            };
            if preparation.advance(store)? {
                return Ok(None);
            }
            self.preparation
                .take()
                .unwrap_or_else(|| unreachable!("finished WMO retains its root"))
                .finish()
                .map(Some)
        })();
        result.map_err(|e| WmoLoadError::Asset(Arc::new(e)))
    }
    /// A failed prerequisite publishes its original error to all joined consumers.
    pub fn fail(mut self, error: WmoLoadError) {
        self.publish(Err(error));
    }
    /// Retire pending authority before waking readers, without holding a cache lock.
    fn publish(&mut self, result: Outcome) {
        let removed = self
            .service
            .0
            .pending
            .lock()
            .unwrap_or_else(|_| unreachable!("WMO request metadata cannot panic"))
            .remove(&self.key);
        assert!(
            removed
                .as_ref()
                .is_some_and(|slot| Arc::ptr_eq(slot, &self.slot)),
            "only the registered WMO producer publishes"
        );
        *self
            .slot
            .result
            .lock()
            .unwrap_or_else(|_| unreachable!("WMO result metadata cannot panic")) = Some(result);
        self.finished = true;
        self.slot.ready.notify_all();
        self.slot.publish_dependencies();
    }
}
impl Drop for WmoLoadProducer {
    fn drop(&mut self) {
        if !self.finished {
            self.publish(Err(WmoLoadError::Abandoned));
        }
    }
}
