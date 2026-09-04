//! Worker-owned archive reads for selected Glue sound requests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use solarity_asset::{ArchiveCatalog, AssetStore};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_media::{EncodedSound, SoundCache, SoundEngine, SoundLoadHandle, SoundLoadRequest};

use super::RuntimeSoundError;

/// One read result whose reservation may have been cancelled while it ran.
pub(super) struct SoundReadCompletion {
    pub(super) handle: SoundLoadHandle,
    pub(super) result: Result<Arc<EncodedSound>, RuntimeSoundError>,
}

/// A single archive owner, lazily mounted once on a CPU worker.
struct SoundReadAssets {
    catalog: ArchiveCatalog,
    store: Option<Result<AssetStore, String>>,
}

impl SoundReadAssets {
    /// Preserves the discovered precedence and never retries a failed mount.
    fn read(&mut self, request: &SoundLoadRequest) -> Result<Arc<EncodedSound>, RuntimeSoundError> {
        let store = self.store.get_or_insert_with(|| {
            AssetStore::mount(self.catalog.clone()).map_err(|error| error.to_string())
        });
        let store = store
            .as_mut()
            .map_err(|message| RuntimeSoundError::LoaderUnavailable {
                message: message.clone(),
            })?;
        SoundCache::new()
            .load(store, request.path())
            .map_err(|error| RuntimeSoundError::Engine(error.into()))
    }
}

/// At most one archive extraction runs; newer requests keep their submission order.
struct ActiveSoundRead {
    handle: SoundLoadHandle,
    task: CpuTask<Result<Arc<EncodedSound>, RuntimeSoundError>>,
}

/// Main-thread scheduler with worker-private assets and explicit shutdown ownership.
pub(super) struct RuntimeSoundLoader {
    // Only a worker takes this lock. The shared owner survives a capacity refusal
    // by CpuExecutor, which otherwise drops the submitted closure and its captures.
    assets: Arc<Mutex<SoundReadAssets>>,
    queued: VecDeque<SoundLoadRequest>,
    active: Option<ActiveSoundRead>,
}

impl RuntimeSoundLoader {
    /// Defers mounting to the first admitted job without changing archive discovery.
    pub(super) fn new(catalog: ArchiveCatalog) -> Self {
        Self {
            assets: Arc::new(Mutex::new(SoundReadAssets {
                catalog,
                store: None,
            })),
            queued: VecDeque::new(),
            active: None,
        }
    }

    /// Takes responsibility for completing or cancelling one engine reservation.
    pub(super) fn queue(&mut self, request: SoundLoadRequest) {
        self.queued.push_back(request);
    }

    /// Collects completed work and submits the oldest still-live request without waiting.
    pub(super) fn poll(
        &mut self,
        cpu: &CpuExecutor,
        engine: &SoundEngine<'_>,
    ) -> Result<Option<SoundReadCompletion>, RuntimeSoundError> {
        let completion = if self
            .active
            .as_ref()
            .is_some_and(|read| read.task.is_finished())
        {
            self.finish_active()?
        } else {
            None
        };
        if completion.is_some() || self.active.is_some() {
            return Ok(completion);
        }
        while self
            .queued
            .front()
            .is_some_and(|load| !engine.is_load_pending(load.handle()))
        {
            self.queued.pop_front();
        }
        let Some(request) = self.queued.front().cloned() else {
            return Ok(completion);
        };
        let handle = request.handle();
        let assets = Arc::clone(&self.assets);
        match cpu.try_submit(move || {
            let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| Instant::now());
            let result = assets.lock()
                .map_err(|_poisoned| RuntimeSoundError::LoaderUnavailable {
                    message: "sound archive owner is poisoned".to_owned(),
                })?
                .read(&request);
            if let Some(start) = timing {
                tracing::info!(path = %request.path(), elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0,
                    "read sound payload on worker");
            }
            result
        }) {
            Ok(task) => {
                self.queued.pop_front();
                self.active = Some(ActiveSoundRead { handle, task });
            }
            // Backpressure leaves the exact selected path and RNG state intact.
            Err(CpuError::AtCapacity { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(completion)
    }

    /// Cancels queued reservations and observes the active job before dropping assets.
    pub(super) fn shutdown(
        &mut self,
        engine: &mut SoundEngine<'_>,
    ) -> Result<(), RuntimeSoundError> {
        for request in self.queued.drain(..) {
            engine.cancel_load(request.handle());
        }
        if let Some(active) = &self.active {
            engine.cancel_load(active.handle);
        }
        if let Some(completion) = self.finish_active()?
            && let Err(error) = completion.result
        {
            tracing::warn!(%error, "cancelled Glue audio read failed");
        }
        Ok(())
    }

    /// Joins only after readiness or at explicit application shutdown.
    fn finish_active(&mut self) -> Result<Option<SoundReadCompletion>, RuntimeSoundError> {
        let Some(read) = self.active.take() else {
            return Ok(None);
        };
        Ok(Some(SoundReadCompletion {
            handle: read.handle,
            result: read.task.join()?,
        }))
    }
}
