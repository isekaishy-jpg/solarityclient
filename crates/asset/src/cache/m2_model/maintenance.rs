//! Namespace-owned cache maintenance is finite work for the application's CPU pool.

use super::super::source_storage::{ControlOwner, RetirementSignal, SourceStorage};
use super::ModelCacheCore;
use crate::{AssetError, AssetReadBudget, AssetStorageVec};
use solarity_cpu::{CpuService, CpuStorageClass, CpuStorageKind};
use std::{
    fmt,
    ops::ControlFlow,
    sync::{Arc, Mutex, atomic::Ordering},
};

/// Registry ownership prevents an expired cache's final payload drop on the frame thread.
pub(super) struct Registry {
    owners: Mutex<OwnerIndex>,
    changed: RetirementSignal,
    pub(super) requests: Mutex<super::requests::RequestIndex>,
    control: ControlOwner,
}

/// Stable slots let maintenance keep a finite cursor without copying the owner list.
/// Empty slots are reused, so capacity follows peak simultaneous cache ownership.
struct OwnerIndex {
    entries: AssetStorageVec<OwnerSlot>,
    free: Option<usize>,
    len: usize,
}
struct OwnerSlot {
    core: Option<Arc<ModelCacheCore>>,
    next_free: Option<usize>,
}
impl Default for OwnerIndex {
    fn default() -> Self {
        Self {
            entries: AssetStorageVec::metadata(),
            free: None,
            len: 0,
        }
    }
}
enum OwnerVisit {
    Active(Arc<ModelCacheCore>),
    Closed(Arc<ModelCacheCore>),
    Empty,
}
impl OwnerIndex {
    fn reserve_one(&mut self, policy: Option<&AssetReadBudget>) -> Result<(), AssetError> {
        if self.free.is_none() {
            self.entries.reserve_one(policy)?;
        } else {
            // A previously offline registry must still adopt its retained allocation.
            self.entries.reserve(policy, self.entries.len())?;
        }
        Ok(())
    }
    fn insert_reserved(&mut self, core: Arc<ModelCacheCore>) {
        if let Some(index) = self.free {
            self.free = self.entries[index].next_free;
            self.entries[index] = OwnerSlot {
                core: Some(core),
                next_free: None,
            };
        } else {
            self.entries.push_reserved(OwnerSlot {
                core: Some(core),
                next_free: None,
            });
        }
        self.len += 1;
    }
    /// Only metadata moves under the registry lock; final owner disposal belongs to the caller.
    fn visit(&mut self, index: usize) -> OwnerVisit {
        let Some(slot) = self.entries.get_mut(index) else {
            return OwnerVisit::Empty;
        };
        let Some(core) = &slot.core else {
            return OwnerVisit::Empty;
        };
        if core.owned.load(Ordering::Acquire) {
            return OwnerVisit::Active(Arc::clone(core));
        }
        let core = slot
            .core
            .take()
            .unwrap_or_else(|| unreachable!("observed registered owner"));
        slot.next_free = self.free;
        self.free = Some(index);
        self.len -= 1;
        OwnerVisit::Closed(core)
    }
}

/// Shared by catalog clones; no worker, timer or thread is created by this service.
#[derive(Clone)]
pub struct M2CacheService(pub(super) Arc<Registry>);

impl Default for M2CacheService {
    fn default() -> Self {
        Self::with_storage(Arc::default())
    }
}

impl fmt::Debug for M2CacheService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("M2CacheService").finish_non_exhaustive()
    }
}

impl M2CacheService {
    pub(crate) fn with_storage(storage: Arc<SourceStorage>) -> Self {
        Self(Arc::new(Registry {
            owners: Mutex::new(OwnerIndex::default()),
            changed: RetirementSignal::new(&storage),
            requests: Mutex::default(),
            control: ControlOwner::for_arc::<Registry>(&storage),
        }))
    }
    /// Binds namespace source ownership to the application storage budget.
    /// Mounted readers use it for required inputs unless a loading scope overrides
    /// the class; retained models and textures keep separate payload charges.
    /// Configure before the first cache load; offline tools may leave it unconfigured.
    /// # Errors
    /// Rejects repeated configuration, an already active source cache, or insufficient
    /// required metadata capacity for the namespace controls. Refusal permits retry.
    pub fn configure_storage(
        &self,
        budget: solarity_cpu::CpuStorageBudget,
    ) -> Result<(), crate::AssetError> {
        let _requests = self
            .0
            .requests
            .lock()
            .unwrap_or_else(|_| unreachable!("request metadata cannot panic"));
        let owners = self
            .0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"));
        if owners.len != 0 {
            return Err(crate::AssetError::SourceStorageConfigured);
        }
        self.0.control.configure(budget)
    }

    pub(crate) fn storage(&self) -> Option<&solarity_cpu::CpuStorageBudget> {
        self.0.control.budget()
    }

    /// First namespace use registers the cache and its durable release-change signal.
    pub(super) fn register(&self, core: &Arc<ModelCacheCore>) -> Result<(), AssetError> {
        let policy = self
            .storage()
            .cloned()
            .map(|storage| AssetReadBudget::for_service(storage, CpuService::Required));
        let mut owners = self
            .0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"));
        owners.reserve_one(policy.as_ref())?;
        {
            let mut cache = core.lock();
            if core.memory.get().is_none()
                && let Some(storage) = self.storage()
            {
                let memory = storage.reserve(
                    CpuStorageClass::Required,
                    CpuStorageKind::Metadata,
                    size_of::<ModelCacheCore>() + 2 * size_of::<usize>(),
                )?;
                core.memory
                    .set(memory)
                    .unwrap_or_else(|_| unreachable!("core admission holds its cache lock"));
            }
            cache.admit(self.storage())?;
            cache.subscribe(&self.0.changed)?;
        }
        // No fallible allocation follows observer registration.
        owners.insert_reserved(Arc::clone(core));
        self.0.changed.store(true, Ordering::Release);
        Ok(())
    }

    /// The single runtime coordinator acknowledges changes before observing deadlines.
    /// A concurrent release remains marked for the following observation.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.0.changed.swap(false, Ordering::AcqRel)
    }

    /// Inspects one head per registered cache only after demand/deadline changes.
    /// The lock order is registry, source index, release metadata. No reverse path
    /// holds an index lock while registering and no source destructor runs here.
    #[must_use]
    pub fn next_collection_delay_ms(&self) -> Option<u32> {
        let owners = self
            .0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"));
        owners
            .entries
            .iter()
            .filter_map(|slot| slot.core.as_ref())
            .filter_map(|core| {
                if core.owned.load(Ordering::Acquire) {
                    core.lock().next_delay_ms()
                } else {
                    Some(0)
                }
            })
            .min()
    }

    /// Starts finite cleanup without allocating or detaching an unbounded list of owners.
    /// Later registrations are covered by the durable change signal and a subsequent pass.
    #[must_use]
    pub fn begin_collection(&self) -> M2CacheCollection {
        let limit = self
            .0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"))
            .entries
            .len();
        M2CacheCollection {
            active: None,
            service: self.clone(),
            cursor: 0,
            limit,
        }
    }
}

/// Each turn inspects one registered slot and destroys at most sixteen detached sources,
/// or one closed cache owner. A source/closed owner can have an indivisible destructor.
/// No temporary owner list or new byte reservation is needed to make cleanup progress.
pub struct M2CacheCollection {
    active: Option<Arc<ModelCacheCore>>,
    service: M2CacheService,
    cursor: usize,
    limit: usize,
}

impl M2CacheCollection {
    /// Advances outside scheduler/registry locks and yields between finite owner batches.
    pub fn step(&mut self) -> ControlFlow<()> {
        if self.cursor >= self.limit {
            return ControlFlow::Break(());
        }
        if self.active.is_none() {
            let owner = self
                .service
                .0
                .owners
                .lock()
                .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"))
                .visit(self.cursor);
            match owner {
                OwnerVisit::Active(core) => self.active = Some(core),
                OwnerVisit::Closed(core) => {
                    drop(core);
                    return self.advance();
                }
                OwnerVisit::Empty => return self.advance(),
            }
        }
        let core = self
            .active
            .as_ref()
            .unwrap_or_else(|| unreachable!("selected live owner"));
        let now = core.lock().collection_time();
        for _ in 0..16 {
            let retired = core.lock().take_unused(now);
            let Some(retired) = retired else {
                return self.advance();
            };
            drop(retired);
        }
        ControlFlow::Continue(())
    }

    fn advance(&mut self) -> ControlFlow<()> {
        // Drop this pass's owner outside every metadata lock.
        self.active = None;
        self.cursor += 1;
        if self.cursor == self.limit {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/cache/model_maintenance.rs"]
mod tests;
