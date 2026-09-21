//! Shared readiness feeds the original voice FIFO without coupling cancellation.

use std::sync::Arc;

use solarity_cpu::{CpuError, CpuExecutor};
use solarity_media::SoundEngine;

use super::{PendingSoundDecode, RuntimeSoundError, RuntimeSoundLoader, SoundLoadCompletion};

impl RuntimeSoundLoader {
    /// Polls finite work and delivers at most one original voice completion.
    /// Cancelled generations withdraw their own pins; other consumers keep the producer.
    pub(in crate::application::sound_coordinator) fn poll(
        &mut self,
        cpu: &CpuExecutor,
        engine: &mut SoundEngine<'_>,
    ) -> Result<Option<SoundLoadCompletion>, RuntimeSoundError> {
        self.retired.extend(self.requests.poll());
        self.queued.retain(|read| {
            if read.request.is_pending() {
                return true;
            }
            if let Some(result) = self.requests.cancel(&read.key, &read.request.handle()) {
                self.retired.extend([result]);
            }
            false
        });
        self.retired.service(cpu)?;

        if self.decoding.is_none()
            && let Some(front) = self.queued.front()
            && let Some(result) = self.requests.ready(&front.key, &front.request.handle())
        {
            let read = self
                .queued
                .pop_front()
                .unwrap_or_else(|| unreachable!("the ready consumer remains at the FIFO head"));
            let handle = read.request.handle();
            match result {
                Ok(encoded) => {
                    self.decoding = Some(PendingSoundDecode {
                        key: read.key,
                        handle,
                        encoded,
                    })
                }
                Err(error) => {
                    engine.cancel_load(handle);
                    if let Some(retired) = self.requests.cancel(&read.key, &handle) {
                        self.retired.extend([retired]);
                    }
                    return Ok(Some(SoundLoadCompletion {
                        handle,
                        result: Err(RuntimeSoundError::SharedSource(error)),
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
            let decode = self
                .decoding
                .take()
                .unwrap_or_else(|| unreachable!("only this poll owns the pending decoder input"));
            let handle = decode.handle;
            if let Some(retired) = self.requests.cancel(&decode.key, &handle) {
                self.retired.extend([retired]);
            }
            self.retired.extend([Ok(decode.encoded)]);
            return Ok(Some(SoundLoadCompletion { handle, result }));
        }

        // A single archive owner serializes physical reads. Shared readiness
        // does not introduce a global archive lock or another hidden worker pool.
        if self.requests.has_running() {
            return Ok(None);
        }
        let Some(front) = self.queued.front() else {
            return Ok(None);
        };
        let assets = &self.assets;
        match self.requests.start(&front.key, cpu, || {
            let request = front.request.clone();
            let assets = Arc::clone(assets);
            let budget = solarity_asset::AssetReadBudget::for_service(
                cpu.storage().clone(),
                solarity_cpu::CpuService::Required,
            );
            move || {
                let _profile = solarity_profiling::profile!("audio.archive.worker");
                assets
                    .lock()
                    .map_err(|_| RuntimeSoundError::LoaderUnavailable {
                        message: "sound archive owner is poisoned".to_owned(),
                    })?
                    .read(&request, &budget)
            }
        }) {
            Ok(_) | Err(CpuError::AtCapacity { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(None)
    }
}
