//! Ordered archive extraction and decoder preparation for selected Glue sounds.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use solarity_asset::{ArchiveCatalog, AssetStore};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_media::{
    EncodedSound, SoundCache, SoundEngine, SoundLoadHandle, SoundLoadRequest, SoundPlayback,
};

use super::RuntimeSoundError;

/// One final admission result whose reservation may have been cancelled while loading.
pub(super) struct SoundLoadCompletion {
    pub(super) handle: SoundLoadHandle,
    pub(super) result: Result<SoundPlayback, RuntimeSoundError>,
}

/// Archive completion stays private until worker decoding has also completed.
struct SoundReadCompletion {
    handle: SoundLoadHandle,
    result: Result<Arc<EncodedSound>, RuntimeSoundError>,
}

/// Retains selected bytes while decoder submission is at capacity or still running.
struct PendingSoundDecode {
    handle: SoundLoadHandle,
    encoded: Arc<EncodedSound>,
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
    decoding: Option<PendingSoundDecode>,
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
            decoding: None,
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
        engine: &mut SoundEngine<'_>,
    ) -> Result<Option<SoundLoadCompletion>, RuntimeSoundError> {
        if self
            .active
            .as_ref()
            .is_some_and(|read| read.task.is_finished())
            && let Some(completion) = self.finish_active()
        {
            match completion.result {
                Ok(encoded) => {
                    self.decoding = Some(PendingSoundDecode {
                        handle: completion.handle,
                        encoded,
                    })
                }
                Err(error) => {
                    engine.cancel_load(completion.handle);
                    return Ok(Some(SoundLoadCompletion {
                        handle: completion.handle,
                        result: Err(error),
                    }));
                }
            }
        }
        if let Some(decode) = &self.decoding {
            let result = match engine.poll_load(cpu, decode.handle, &decode.encoded) {
                Ok(None) => return Ok(None),
                Ok(Some(playback)) => Ok(playback),
                Err(error) => Err(error.into()),
            };
            let handle = decode.handle;
            self.decoding = None;
            return Ok(Some(SoundLoadCompletion { handle, result }));
        }
        if self.active.is_some() {
            return Ok(None);
        }
        while self
            .queued
            .front()
            .is_some_and(|load| !engine.is_load_pending(load.handle()))
        {
            self.queued.pop_front();
        }
        let Some(request) = self.queued.front().cloned() else {
            return Ok(None);
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
        Ok(None)
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
        if let Some(decode) = self.decoding.take() {
            engine.cancel_load(decode.handle);
        }
        let completion = self.finish_active();
        engine.finish_cancelled_loads();
        if let Some(completion) = completion
            && let Err(error) = completion.result
        {
            tracing::warn!(%error, "cancelled Glue audio read failed");
        }
        Ok(())
    }

    /// Joins only after readiness or at explicit application shutdown.
    fn finish_active(&mut self) -> Option<SoundReadCompletion> {
        let read = self.active.take()?;
        Some(SoundReadCompletion {
            handle: read.handle,
            // A worker failure still belongs to this reservation. Publishing
            // it lets the coordinator retire the pending voice exactly once.
            result: read
                .task
                .join()
                .map_err(RuntimeSoundError::from)
                .and_then(|result| result),
        })
    }
}
