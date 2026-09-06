//! Ordered sound selection separated from nonblocking archive reads and decoding.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use solarity_asset::{AssetPath, AssetStore};
use solarity_cpu::CpuExecutor;

use crate::audio::backend::SoundVoicePriority;
use crate::audio::cache::EncodedSound;
use crate::audio::codec::{DecodedSoundHandle, SoundDecodeAdmission, SoundDecodeTicket};
use crate::audio::engine::{AdvancedSoundInstanceId, SoundLoopMode, SoundResidencyPolicy};
use crate::audio::selection::SoundVariationSelector;

use super::{
    ActiveVoice, SoundChannel, SoundEngine, SoundEngineError, SoundPlayRequest, SoundPlayback,
};

/// Process-unique identity for a selected sound awaiting resource admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundLoadHandle(u64);

/// Exact selected path that a worker may read without access to sound or RNG state.
#[derive(Clone, Debug)]
pub struct SoundLoadRequest {
    handle: SoundLoadHandle,
    path: AssetPath,
}

impl SoundLoadRequest {
    /// Allocates a non-reusable identity so foreign and late completions cannot play.
    fn new(path: AssetPath) -> Result<Self, SoundEngineError> {
        static NEXT_LOAD_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_LOAD_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_exhausted| SoundEngineError::LoadCapacity)?;
        Ok(Self {
            handle: SoundLoadHandle(id),
            path,
        })
    }

    /// Returns the identity used for completion and cancellation.
    #[must_use]
    pub const fn handle(&self) -> SoundLoadHandle {
        self.handle
    }

    /// Returns the one selected archive path; workers must not choose another variation.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }
}

/// Admission policy retained while bytes are read, before an SDL track exists.
pub(super) struct PendingVoice {
    load: SoundLoadRequest,
    pub(super) entry_id: Option<u32>,
    pub(super) channel: SoundChannel,
    source_gain: f32,
    looping: bool,
    priority: SoundVoicePriority,
    duck_source: Option<AdvancedSoundInstanceId>,
    residency: SoundResidencyPolicy,
    pub(super) decode: Option<SoundDecodeTicket>,
}

impl SoundEngine<'_> {
    /// Selects and reserves a sound without reading or decoding its payload.
    ///
    /// Build 12340 `SoundEngine.cpp` at `0x0087ee60` passes `0x82010000`,
    /// including `FMOD_NONBLOCKING`, to sample and stream creation. Selection,
    /// RNG, category suppression, exclusivity, and channel limits remain ordered
    /// here. Pending voices count toward admission limits until completion or
    /// cancellation. Callers must complete or cancel every returned request.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for invalid definitions or exhausted admission.
    pub fn begin_load(
        &mut self,
        request: SoundPlayRequest,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<Option<SoundLoadRequest>, SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        let entry =
            self.catalog
                .sound_entry(request.entry_id())
                .ok_or(SoundEngineError::MissingEntry {
                    entry_id: request.entry_id(),
                })?;
        let channel = request.channel();
        let category = channel.category();
        let Some(_category_gain) = self.settings.category_gain(category) else {
            return Ok(None);
        };
        if !entry.volume().is_finite() || entry.volume() < 0.0 {
            return Err(SoundEngineError::InvalidEntryVolume {
                entry_id: entry.id(),
                volume: entry.volume(),
            });
        }
        if let Some(maximum) = channel.maximum_active_voices()
            && self
                .active_voices
                .iter()
                .filter(|voice| voice.channel == channel)
                .count()
                + self
                    .pending_voices
                    .iter()
                    .filter(|voice| voice.channel == channel)
                    .count()
                >= maximum
        {
            return Err(SoundEngineError::ChannelCapacity {
                channel: channel.value(),
                maximum,
            });
        }
        if request.concurrency_mode().is_exclusive(entry.flags())
            && (self
                .active_voices
                .iter()
                .any(|voice| voice.entry_id == Some(entry.id()))
                || self
                    .pending_voices
                    .iter()
                    .any(|voice| voice.entry_id == Some(entry.id())))
        {
            return Err(SoundEngineError::ExclusiveEntryActive {
                entry_id: entry.id(),
            });
        }
        let selector_index = match self
            .variation_selectors
            .binary_search_by_key(&entry.id(), |(entry_id, _selector)| *entry_id)
        {
            Ok(index) => index,
            Err(index) => {
                let selector = SoundVariationSelector::new(entry).ok_or(
                    SoundEngineError::NoPlayableVariation {
                        entry_id: entry.id(),
                    },
                )?;
                self.variation_selectors
                    .insert(index, (entry.id(), selector));
                index
            }
        };
        let asset_path = self.variation_selectors[selector_index]
            .1
            .select(request.variation_mode(), next_random_word)
            .ok_or(SoundEngineError::NoPlayableVariation {
                entry_id: entry.id(),
            })?
            .path()
            .clone();
        let pending = PendingVoice {
            load: SoundLoadRequest::new(asset_path)?,
            entry_id: Some(entry.id()),
            channel,
            source_gain: entry.volume(),
            looping: request.loop_mode().is_looping(entry.flags()),
            priority: request.priority(),
            duck_source: request.advanced_source(),
            residency: self.settings.residency(),
            decode: None,
        };
        let load = pending.load.clone();
        self.pending_voices.push(pending);
        Ok(Some(load))
    }

    /// Reserves an exact direct-path sound without archive access.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for channel or request-identity exhaustion.
    pub fn begin_file_load(
        &mut self,
        path: &AssetPath,
        channel: SoundChannel,
        loop_mode: SoundLoopMode,
    ) -> Result<Option<SoundLoadRequest>, SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        if self.settings.category_gain(channel.category()).is_none() {
            return Ok(None);
        }
        if let Some(maximum) = channel.maximum_active_voices()
            && self
                .active_voices
                .iter()
                .filter(|voice| voice.channel == channel)
                .count()
                + self
                    .pending_voices
                    .iter()
                    .filter(|voice| voice.channel == channel)
                    .count()
                >= maximum
        {
            return Err(SoundEngineError::ChannelCapacity {
                channel: channel.value(),
                maximum,
            });
        }
        let load = SoundLoadRequest::new(path.clone())?;
        self.pending_voices.push(PendingVoice {
            load: load.clone(),
            entry_id: None,
            channel,
            source_gain: 1.0,
            looping: loop_mode.is_looping(0),
            priority: SoundVoicePriority::DEFAULT,
            duck_source: None,
            residency: self.settings.residency(),
            decode: None,
        });
        Ok(Some(load))
    }

    /// Reports whether a worker completion still belongs to a live request.
    #[must_use]
    pub fn is_load_pending(&self, handle: SoundLoadHandle) -> bool {
        self.pending_voices
            .iter()
            .any(|voice| voice.load.handle == handle)
    }

    /// Retires a reservation after cancellation or failed archive extraction.
    ///
    /// Already completed, foreign, and previously cancelled identities return false.
    pub fn cancel_load(&mut self, handle: SoundLoadHandle) -> bool {
        let Some(index) = self
            .pending_voices
            .iter()
            .position(|voice| voice.load.handle == handle)
        else {
            return false;
        };
        let pending = self.pending_voices.remove(index);
        if let Some(ticket) = pending.decode {
            self.decoder.cancel_load(ticket);
        }
        true
    }

    /// Waits for already cancelled decoder jobs before application services shut down.
    ///
    /// Ordinary cancellation is nonblocking; this explicit lifecycle boundary
    /// joins those jobs and discards their results while SDL remains initialized.
    pub fn finish_cancelled_loads(&mut self) {
        self.decoder.finish_cancelled_loads();
    }

    /// Polls nonblocking decode and playback admission for the exact selected payload.
    ///
    /// `None` means the CPU pool is full or the decode is still running. Keep
    /// the selected bytes and call again, without another selection or random
    /// draw. A cached sample can complete immediately. New SDL resources remain
    /// owned by this engine even when the request is cancelled during a decode.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for mismatched bytes, worker/decode failure,
    /// or playback admission failure. Every error retires the reservation.
    pub fn poll_load(
        &mut self,
        cpu: &CpuExecutor,
        handle: SoundLoadHandle,
        encoded: &Arc<EncodedSound>,
    ) -> Result<Option<SoundPlayback>, SoundEngineError> {
        let Some(index) = self
            .pending_voices
            .iter()
            .position(|voice| voice.load.handle == handle)
        else {
            return Ok(Some(SoundPlayback::Suppressed));
        };
        if encoded.path() != &self.pending_voices[index].load.path {
            let expected = self.pending_voices[index].load.path.clone();
            self.cancel_load(handle);
            return Err(SoundEngineError::LoadPathMismatch {
                expected,
                actual: encoded.path().clone(),
            });
        }
        let decoded = if let Some(ticket) = self.pending_voices[index].decode {
            self.decoder.poll_load(ticket)
        } else {
            let mode = self.pending_voices[index]
                .residency
                .decode_mode(encoded.path(), encoded.bytes().len());
            self.decoder
                .begin_load(cpu, encoded, mode)
                .map(|admission| match admission {
                    SoundDecodeAdmission::Ready(sound) => Some(sound),
                    SoundDecodeAdmission::Pending(ticket) => {
                        self.pending_voices[index].decode = Some(ticket);
                        None
                    }
                    SoundDecodeAdmission::AtCapacity => None,
                })
        };
        let sound = match decoded {
            Ok(Some(sound)) => sound,
            Ok(None) => return Ok(None),
            Err(error) => {
                self.cancel_load(handle);
                return Err(error.into());
            }
        };
        let pending = self.pending_voices.remove(index);
        self.start_decoded_voice(pending, sound).map(Some)
    }

    /// Admits the exact completed payload on the output-owning thread.
    ///
    /// Late, foreign, or cancelled completions are suppressed before decoding.
    /// Every live completion consumes its reservation, including a failed decode.
    /// Live gain settings apply at playback; disabling a category after selection
    /// mutes the accepted voice just as it mutes an already playing voice.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for a mismatched path, decode failure, or
    /// backend admission failure. No substitute asset or voice is requested.
    pub fn complete_load(
        &mut self,
        handle: SoundLoadHandle,
        encoded: &EncodedSound,
    ) -> Result<SoundPlayback, SoundEngineError> {
        let Some(index) = self
            .pending_voices
            .iter()
            .position(|voice| voice.load.handle == handle)
        else {
            return Ok(SoundPlayback::Suppressed);
        };
        let pending = self.pending_voices.remove(index);
        if encoded.path() != &pending.load.path {
            if let Some(ticket) = pending.decode {
                self.decoder.cancel_load(ticket);
            }
            return Err(SoundEngineError::LoadPathMismatch {
                expected: pending.load.path,
                actual: encoded.path().clone(),
            });
        }
        let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| Instant::now());
        let decode_mode = pending
            .residency
            .decode_mode(encoded.path(), encoded.bytes().len());
        let sound = if let Some(ticket) = pending.decode {
            self.decoder.finish_load(ticket)?
        } else {
            self.decoder.load(encoded, decode_mode)?
        };
        let decode_elapsed = timing.map(|start| start.elapsed());
        let playback = self.start_decoded_voice(pending, sound)?;
        if let (Some(start), Some(decode_elapsed)) = (timing, decode_elapsed) {
            tracing::info!(path = %encoded.path(), encoded_bytes = encoded.bytes().len(), ?decode_mode,
                decode_ms = decode_elapsed.as_secs_f64() * 1_000.0,
                backend_ms = (start.elapsed() - decode_elapsed).as_secs_f64() * 1_000.0,
                "admitted sound resource");
        }
        Ok(playback)
    }

    /// Applies current output policy after either immediate or worker decoding.
    fn start_decoded_voice(
        &mut self,
        pending: PendingVoice,
        sound: DecodedSoundHandle,
    ) -> Result<SoundPlayback, SoundEngineError> {
        let category = pending.channel.category();
        let gain = self.settings.category_gain(category).unwrap_or(0.0) * pending.source_gain;
        let playback = match self.backend.play(
            &self.decoder,
            sound,
            gain,
            pending.looping,
            pending.priority,
        ) {
            Ok(playback) => playback,
            Err(error) => {
                self.decoder.release(sound);
                return Err(error.into());
            }
        };
        if let Some(replaced) = playback.replaced()
            && let Some(index) = self
                .active_voices
                .iter()
                .position(|voice| voice.handle == replaced)
        {
            let replaced_voice = self.active_voices.remove(index);
            self.decoder.release(replaced_voice.sound);
        }
        let voice = playback.voice();
        self.active_voices.push(ActiveVoice {
            handle: voice,
            sound,
            entry_id: pending.entry_id,
            channel: pending.channel,
            category,
            source_gain: pending.source_gain,
            runtime_gain: 1.0,
            duck_gain: 1.0,
            duck_source: pending.duck_source,
        });
        Ok(SoundPlayback::Started(voice))
    }

    /// Completes the synchronous API through the same reservation and cleanup rules.
    pub(super) fn load_immediate(
        &mut self,
        store: &mut AssetStore,
        load: SoundLoadRequest,
    ) -> Result<SoundPlayback, SoundEngineError> {
        let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| Instant::now());
        let encoded = match self.cache.load(store, load.path()) {
            Ok(encoded) => encoded,
            Err(error) => {
                self.cancel_load(load.handle());
                return Err(error.into());
            }
        };
        if let Some(start) = timing {
            tracing::info!(path = %load.path(), elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0,
                "read sound payload");
        }
        self.complete_load(load.handle(), &encoded)
    }
}
