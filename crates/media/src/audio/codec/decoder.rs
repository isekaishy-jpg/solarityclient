//! SDL3_mixer resource admission from archive-selected memory payloads.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sdl3::mixer::Audio;
use solarity_asset::AssetPath;
use solarity_cpu::CpuExecutor;

use crate::audio::cache::EncodedSound;

use super::loader::{PreparedSound, SoundDecodeTicket, SoundLoader};
use super::status::SoundDecodeError;
use super::types::{DecodedSoundHandle, DecodedSoundInfo, SoundDecodeMode};

/// A retained sample is ready immediately; new resources use bounded CPU work.
pub(in crate::audio) enum SoundDecodeAdmission {
    Ready(DecodedSoundHandle),
    Pending(SoundDecodeTicket),
    AtCapacity,
}

/// Resource identity includes decode mode because it changes retained storage.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DecodedSoundKey {
    path: AssetPath,
    mode: SoundDecodeMode,
}

/// One SDL-owned audio object and its dependency-neutral description.
struct DecodedSoundResource {
    audio: Audio,
    info: DecodedSoundInfo,
    encoded_size_bytes: usize,
    references: usize,
}

/// Single-threaded decoded-audio registry independent of an output device.
///
/// SDL documents `Audio` objects as shareable across mixer devices. Loading
/// against a memory mixer admits reusable predecoded samples before a playback
/// engine creates its output tracks. Stock streaming admission remains
/// per-play and is not inserted into that reusable sample map.
pub struct SoundDecoder {
    decoder_id: u64,
    handles: HashMap<DecodedSoundKey, DecodedSoundHandle>,
    resources: Vec<Option<DecodedSoundResource>>,
    resource_count: usize,
    cached_sample_bytes: usize,
    // Loaded Audio objects and pending worker results must retire before the
    // memory mixer releases its SDL_mixer library initialization reference.
    loader: SoundLoader,
}

impl SoundDecoder {
    /// Initializes SDL3_mixer and a device-independent memory mixer.
    ///
    /// # Errors
    ///
    /// Returns [`SoundDecodeError`] when SDL3_mixer cannot initialize or create
    /// the memory mixer.
    pub fn new() -> Result<Self, SoundDecodeError> {
        static NEXT_DECODER_ID: AtomicU64 = AtomicU64::new(1);
        Ok(Self {
            decoder_id: NEXT_DECODER_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            resource_count: 0,
            cached_sample_bytes: 0,
            loader: SoundLoader::new()?,
        })
    }

    /// Returns the number of retained samples and live per-play streams.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resource_count
    }

    /// Reports whether no SDL audio resources are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resource_count == 0
    }

    /// Returns the encoded-byte accounting used by the stock sample budget.
    #[must_use]
    pub const fn cached_sample_bytes(&self) -> usize {
        self.cached_sample_bytes
    }

    /// Admits one encoded payload in the caller-selected residency mode.
    ///
    /// SDL3_mixer copies the original encoded file during this call. The input
    /// stream can retire immediately. Subsequent predecoded requests for the
    /// same normalized path reuse the sample; streaming requests create one
    /// distinct resource per play, matching the stock noncacheable branch.
    ///
    /// # Errors
    ///
    /// Returns [`SoundDecodeError`] for empty/unsupported encoded data, decoder
    /// failures, invalid reported formats, or handle-capacity exhaustion.
    pub fn load(
        &mut self,
        encoded: &EncodedSound,
        mode: SoundDecodeMode,
    ) -> Result<DecodedSoundHandle, SoundDecodeError> {
        if let Some(handle) = self.retain_sample(encoded.path(), mode)? {
            return Ok(handle);
        }
        let prepared = self.loader.prepare(encoded, mode)?;
        self.admit(prepared)
    }

    /// Starts decoding without blocking; an existing sample acquires its reference now.
    pub(in crate::audio) fn begin_load(
        &mut self,
        cpu: &CpuExecutor,
        encoded: &Arc<EncodedSound>,
        mode: SoundDecodeMode,
    ) -> Result<SoundDecodeAdmission, SoundDecodeError> {
        if let Some(handle) = self.retain_sample(encoded.path(), mode)? {
            return Ok(SoundDecodeAdmission::Ready(handle));
        }
        Ok(match self.loader.submit(cpu, Arc::clone(encoded), mode)? {
            Some(ticket) => SoundDecodeAdmission::Pending(ticket),
            None => SoundDecodeAdmission::AtCapacity,
        })
    }

    /// Publishes only a finished worker result to the output-thread registry.
    pub(in crate::audio) fn poll_load(
        &mut self,
        ticket: SoundDecodeTicket,
    ) -> Result<Option<DecodedSoundHandle>, SoundDecodeError> {
        self.loader
            .poll(ticket)?
            .map(|prepared| self.admit(prepared))
            .transpose()
    }

    /// Joins an existing decode when a caller explicitly switches to synchronous completion.
    pub(in crate::audio) fn finish_load(
        &mut self,
        ticket: SoundDecodeTicket,
    ) -> Result<DecodedSoundHandle, SoundDecodeError> {
        let prepared = self.loader.finish(ticket)?;
        self.admit(prepared)
    }

    /// Preserves ownership of cancelled work until its result can be discarded.
    pub(in crate::audio) fn cancel_load(&mut self, ticket: SoundDecodeTicket) {
        self.loader.cancel(ticket);
    }

    /// Drains cancelled work at the runtime's explicit shutdown boundary.
    pub(in crate::audio) fn finish_cancelled_loads(&mut self) {
        self.loader.finish_cancelled();
    }

    /// Reuses samples only; streamed voices always retain their own resource.
    fn retain_sample(
        &mut self,
        path: &AssetPath,
        mode: SoundDecodeMode,
    ) -> Result<Option<DecodedSoundHandle>, SoundDecodeError> {
        let key = DecodedSoundKey {
            path: path.clone(),
            mode,
        };
        if mode == SoundDecodeMode::Predecoded
            && let Some(handle) = self.handles.get(&key).copied()
        {
            let Some(resource) = self.resources[handle.slot as usize].as_mut() else {
                return Err(SoundDecodeError::MissingRetainedSample {
                    path: key.path.clone(),
                });
            };
            resource.references = resource
                .references
                .checked_add(1)
                .ok_or(SoundDecodeError::Capacity)?;
            return Ok(Some(handle));
        }
        Ok(None)
    }

    /// Admits completed bytes, rechecking a sample another request could have loaded.
    fn admit(&mut self, prepared: PreparedSound) -> Result<DecodedSoundHandle, SoundDecodeError> {
        let mode = prepared.info.mode();
        if let Some(handle) = self.retain_sample(prepared.info.path(), mode)? {
            return Ok(handle);
        }
        let key = DecodedSoundKey {
            path: prepared.info.path().clone(),
            mode,
        };
        let slot =
            u32::try_from(self.resources.len()).map_err(|_source| SoundDecodeError::Capacity)?;
        let handle = DecodedSoundHandle {
            decoder_id: self.decoder_id,
            slot,
        };
        let encoded_size_bytes = prepared.encoded_size_bytes;
        let next_cached_sample_bytes = if mode == SoundDecodeMode::Predecoded {
            self.cached_sample_bytes
                .checked_add(encoded_size_bytes)
                .ok_or(SoundDecodeError::Capacity)?
        } else {
            self.cached_sample_bytes
        };
        self.resources.push(Some(DecodedSoundResource {
            audio: prepared.audio,
            info: prepared.info,
            encoded_size_bytes,
            references: 1,
        }));
        self.resource_count += 1;
        if mode == SoundDecodeMode::Predecoded {
            self.cached_sample_bytes = next_cached_sample_bytes;
            self.handles.insert(key, handle);
        }
        Ok(handle)
    }

    /// Releases one playback reference after its backend track is clear.
    ///
    /// A stream resource retires with its sole reference. A predecoded sample
    /// remains cached with zero references until the stock byte-budget pass
    /// evicts it. Foreign handles and duplicate releases return `false`.
    pub fn release(&mut self, handle: DecodedSoundHandle) -> bool {
        if handle.decoder_id != self.decoder_id {
            return false;
        }
        let Some(resource) = self
            .resources
            .get_mut(handle.slot as usize)
            .and_then(Option::as_mut)
        else {
            return false;
        };
        if resource.references == 0 {
            return false;
        }
        resource.references -= 1;
        if resource.info.mode() == SoundDecodeMode::Streaming {
            debug_assert_eq!(resource.references, 0);
            self.resources[handle.slot as usize] = None;
            self.resource_count -= 1;
        }
        true
    }

    /// Evicts unreferenced predecoded samples until the byte budget is met.
    ///
    /// Active samples remain resident even when they alone exceed the budget.
    pub fn trim_predecoded_cache(&mut self, maximum_bytes: usize) -> usize {
        self.loader.collect_cancelled();
        let mut removed = 0;
        for slot in 0..self.resources.len() {
            if self.cached_sample_bytes <= maximum_bytes {
                break;
            }
            let Some(resource) = self.resources[slot].as_ref() else {
                continue;
            };
            if resource.info.mode() != SoundDecodeMode::Predecoded || resource.references != 0 {
                continue;
            }
            let key = DecodedSoundKey {
                path: resource.info.path().clone(),
                mode: SoundDecodeMode::Predecoded,
            };
            let encoded_size_bytes = resource.encoded_size_bytes;
            self.handles.remove(&key);
            self.resources[slot] = None;
            self.resource_count -= 1;
            self.cached_sample_bytes -= encoded_size_bytes;
            removed += 1;
        }
        removed
    }

    /// Resolves diagnostics only for handles created by this decoder.
    #[must_use]
    pub fn info(&self, handle: DecodedSoundHandle) -> Option<&DecodedSoundInfo> {
        if handle.decoder_id != self.decoder_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .and_then(Option::as_ref)
            .map(|resource| &resource.info)
    }

    /// Resolves the SDL resource only inside the media crate's backend adapter.
    pub(in crate::audio) fn audio(&self, handle: DecodedSoundHandle) -> Option<&Audio> {
        if handle.decoder_id != self.decoder_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .and_then(Option::as_ref)
            .map(|resource| &resource.audio)
    }
}
