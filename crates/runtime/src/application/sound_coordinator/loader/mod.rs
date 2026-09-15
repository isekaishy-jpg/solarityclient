//! Shared archive reads with independently ordered voice preparation and publication.

mod archives;
mod schedule;
mod shutdown;

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use solarity_asset::{ArchiveCatalog, AssetNamespaceId, AssetResourceKey};
use solarity_cpu::CpuService;
use solarity_media::{EncodedSound, SoundLoadHandle, SoundLoadRequest, SoundPlayback};

use super::RuntimeSoundError;
use crate::application::{
    cpu_retirement::CpuRetirementQueue,
    resource_requests::{ResourceRequests, SharedResult},
};
use archives::SoundReadAssets;

#[cfg(test)]
#[path = "../../../../tests/application/shared_sound_reads.rs"]
mod tests;

/// One final admission result still belongs to its original voice generation.
pub(in crate::application::sound_coordinator) struct SoundLoadCompletion {
    pub(in crate::application::sound_coordinator) handle: SoundLoadHandle,
    pub(in crate::application::sound_coordinator) result: Result<SoundPlayback, RuntimeSoundError>,
}

/// Ordered voice ownership stays separate from the shared archive request key.
struct QueuedSoundRead {
    request: SoundLoadRequest,
    key: AssetResourceKey,
}

/// Retains selected bytes while decoder submission is at capacity or still running.
struct PendingSoundDecode {
    key: AssetResourceKey,
    handle: SoundLoadHandle,
    encoded: Arc<EncodedSound>,
}

/// One index joins pending requests across all coordinator-owned sound channels.
/// Decoder/voice policy remains in SoundEngine; only immutable encoded bytes share.
pub(in crate::application::sound_coordinator) struct RuntimeSoundLoader {
    namespace: AssetNamespaceId,
    assets: Arc<Mutex<SoundReadAssets>>,
    queued: VecDeque<QueuedSoundRead>,
    requests: ResourceRequests<AssetResourceKey, SoundLoadHandle, EncodedSound, RuntimeSoundError>,
    decoding: Option<PendingSoundDecode>,
    retired: CpuRetirementQueue<SharedResult<EncodedSound, RuntimeSoundError>>,
}

impl RuntimeSoundLoader {
    /// Defers mounting to the first admitted job and retains the exact namespace.
    pub(in crate::application::sound_coordinator) fn new(catalog: ArchiveCatalog) -> Self {
        Self {
            namespace: catalog.namespace(),
            assets: Arc::new(Mutex::new(SoundReadAssets {
                catalog,
                store: None,
            })),
            queued: VecDeque::new(),
            requests: ResourceRequests::new(),
            decoding: None,
            retired: CpuRetirementQueue::new(),
        }
    }

    /// Registers the already-selected generation without selecting another path or RNG draw.
    pub(in crate::application::sound_coordinator) fn queue(&mut self, request: SoundLoadRequest) {
        let key = AssetResourceKey::new(self.namespace, request.path().clone());
        self.requests
            .request(key.clone(), request.handle(), CpuService::Required);
        self.queued.push_back(QueuedSoundRead { request, key });
    }
}
