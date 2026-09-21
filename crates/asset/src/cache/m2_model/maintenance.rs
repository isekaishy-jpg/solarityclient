//! Namespace-owned cache maintenance is finite work for the application's CPU pool.

use super::ModelCacheCore;
use std::{
    fmt,
    ops::ControlFlow,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// Registry ownership prevents an expired cache's final payload drop on the frame thread.
#[derive(Default)]
pub(super) struct Registry {
    owners: Mutex<Vec<Arc<ModelCacheCore>>>,
    changed: Arc<AtomicBool>,
    pub(super) requests: Mutex<super::requests::RequestIndex>,
    storage: std::sync::OnceLock<solarity_cpu::CpuStorageBudget>,
}

/// Shared by catalog clones; no worker, timer or thread is created by this service.
#[derive(Clone, Default)]
pub struct M2CacheService(pub(super) Arc<Registry>);

impl fmt::Debug for M2CacheService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("M2CacheService").finish_non_exhaustive()
    }
}

impl M2CacheService {
    /// Binds retained decoded generations to the application's required-result budget.
    /// Configure before the first cache load; offline tools may leave it unconfigured.
    /// # Errors
    /// Rejects repeated configuration or an already active source cache.
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
        if !owners.is_empty() {
            return Err(crate::AssetError::SourceStorageConfigured);
        }
        self.0
            .storage
            .set(budget)
            .map_err(|_| crate::AssetError::SourceStorageConfigured)
    }

    pub(crate) fn storage(&self) -> Option<&solarity_cpu::CpuStorageBudget> {
        self.0.storage.get()
    }

    /// First namespace use registers the cache and its durable release-change signal.
    pub(super) fn register(&self, core: &Arc<ModelCacheCore>) {
        core.lock().subscribe(&self.0.changed);
        self.0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"))
            .push(Arc::clone(core));
        self.0.changed.store(true, Ordering::Release);
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
            .iter()
            .filter_map(|core| {
                if core.owned.load(Ordering::Acquire) {
                    core.lock().next_delay_ms()
                } else {
                    Some(0)
                }
            })
            .min()
    }

    /// Starts finite cleanup on the admitted worker, retaining all owners through its steps.
    #[must_use]
    pub fn begin_collection(&self) -> M2CacheCollection {
        let mut owners = self
            .0
            .owners
            .lock()
            .unwrap_or_else(|_| unreachable!("cache registry metadata cannot panic"));
        let active = owners
            .iter()
            .filter(|core| core.owned.load(Ordering::Acquire))
            .cloned()
            .collect();
        let mut orphaned = Vec::new();
        let mut index = 0;
        while index < owners.len() {
            if owners[index].owned.load(Ordering::Acquire) {
                index += 1;
            } else {
                orphaned.push(owners.swap_remove(index));
            }
        }
        M2CacheCollection {
            active,
            orphaned,
            cursor: 0,
        }
    }
}

/// Each turn destroys at most sixteen detached sources, or one closed cache owner.
/// One source/closed owner may still have an indivisible allocator destructor.
pub struct M2CacheCollection {
    active: Vec<Arc<ModelCacheCore>>,
    orphaned: Vec<Arc<ModelCacheCore>>,
    cursor: usize,
}

impl M2CacheCollection {
    /// Advances outside scheduler/registry locks and yields between finite owner batches.
    pub fn step(&mut self) -> ControlFlow<()> {
        if let Some(owner) = self.orphaned.pop() {
            drop(owner);
            return ControlFlow::Continue(());
        }
        let Some(core) = self.active.get(self.cursor) else {
            return ControlFlow::Break(());
        };
        let now = core.lock().collection_time();
        for _ in 0..16 {
            let retired = core.lock().take_unused(now);
            let Some(retired) = retired else {
                self.cursor += 1;
                return if self.cursor == self.active.len() {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                };
            };
            drop(retired);
        }
        ControlFlow::Continue(())
    }
}
