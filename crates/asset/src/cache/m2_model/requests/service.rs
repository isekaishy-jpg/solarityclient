//! Namespace requests join pending ownership before CPU inputs move.

use super::super::super::resource::{ResourceCache, ResourceCacheClock};
use super::super::{M2ModelCache, ModelCacheCore};
use super::{M2CacheService, M2Load, M2LoadProducer, M2LoadRequest, Slot};
use crate::{AssetError, AssetResourceKey};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

impl M2CacheService {
    /// Joins and promotes an existing producer without requiring another CPU admission slot.
    /// No producer or source cache is created when the key has no pending work.
    /// # Errors
    /// Rejects unsupported model extensions.
    pub fn join_pending(
        &self,
        key: &AssetResourceKey,
        service: solarity_cpu::CpuService,
    ) -> Result<Option<M2LoadRequest>, AssetError> {
        let key = AssetResourceKey::new(key.namespace(), M2ModelCache::canonical_path(key.path())?);
        let index = self
            .0
            .requests
            .lock()
            .unwrap_or_else(|_| unreachable!("model request metadata cannot panic"));
        Ok(index
            .pending
            .get(&key)
            .map(|slot| M2LoadRequest::new(Arc::clone(slot), service)))
    }

    /// Joins namespace/path identity before any archive work or CPU input transfer.
    /// This table's lock may acquire source metadata, but neither decoding nor
    /// final source destruction runs under it. Maintenance never takes this table lock.
    /// # Errors
    /// Rejects unsupported model extensions or exhausted release identity.
    pub fn request(&self, key: &AssetResourceKey) -> Result<M2Load, AssetError> {
        self.request_for(key, solarity_cpu::CpuService::Required)
    }

    /// Joins explicit live demand without promoting a speculative-only consumer.
    /// # Errors
    /// Rejects unsupported model extensions or exhausted release identity.
    pub fn request_for(
        &self,
        key: &AssetResourceKey,
        service: solarity_cpu::CpuService,
    ) -> Result<M2Load, AssetError> {
        let key = AssetResourceKey::new(key.namespace(), M2ModelCache::canonical_path(key.path())?);
        let mut index = self
            .0
            .requests
            .lock()
            .unwrap_or_else(|_| unreachable!("model request metadata cannot panic"));
        if index.core.is_none() {
            let core = Arc::new(ModelCacheCore {
                models: Mutex::new(ResourceCache::with_retention(
                    ResourceCacheClock::monotonic(),
                )),
                owned: AtomicBool::new(true),
            });
            self.register(&core);
            index.core = Some(core);
        }
        if let Some(model) = index
            .core
            .as_ref()
            .unwrap_or_else(|| unreachable!("shared source cache was initialized"))
            .lock()
            .get(&key)?
        {
            return Ok(M2Load::Ready(model));
        }
        if let Some(slot) = index.pending.get(&key) {
            return Ok(M2Load::Pending(M2LoadRequest::new(
                Arc::clone(slot),
                service,
            )));
        }
        let slot = Arc::new(Slot::default());
        index.pending.insert(key.clone(), Arc::clone(&slot));
        Ok(M2Load::Producer(M2LoadProducer {
            service: self.clone(),
            key,
            slot,
            finished: false,
        }))
    }
}
