//! Explicit audio shutdown observes producers before discarding source owners.

use solarity_media::SoundEngine;

use super::{RuntimeSoundError, RuntimeSoundLoader};
use crate::application::cpu_retirement::CpuRetirementQueue;

impl RuntimeSoundLoader {
    /// Cancels every reservation, drains admitted reads and observes each producer
    /// failure once. Synchronous disposal here is explicit process teardown only.
    pub(in crate::application::sound_coordinator) fn shutdown(
        &mut self,
        engine: &mut SoundEngine<'_>,
    ) -> Result<(), RuntimeSoundError> {
        for read in self.queued.drain(..) {
            engine.cancel_load(read.request.handle());
        }
        if let Some(decode) = self.decoding.take() {
            engine.cancel_load(decode.handle);
        }
        for result in self.requests.drain() {
            if let Err(error) = result {
                tracing::warn!(%error, "cancelled Glue audio read failed");
            }
        }
        engine.finish_cancelled_loads();
        self.retired = CpuRetirementQueue::new();
        Ok(())
    }
}
