//! Ordered sound selection separated from nonblocking archive reads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use solarity_asset::{AssetPath, AssetStore};

use crate::audio::backend::SoundVoicePriority;
use crate::audio::cache::EncodedSound;
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
        let before = self.pending_voices.len();
        self.pending_voices
            .retain(|voice| voice.load.handle != handle);
        self.pending_voices.len() != before
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
            return Err(SoundEngineError::LoadPathMismatch {
                expected: pending.load.path,
                actual: encoded.path().clone(),
            });
        }
        let timing = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| Instant::now());
        let decode_mode = pending
            .residency
            .decode_mode(encoded.path(), encoded.bytes().len());
        let sound = self.decoder.load(encoded, decode_mode)?;
        let decode_elapsed = timing.map(|start| start.elapsed());
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
        if let (Some(start), Some(decode_elapsed)) = (timing, decode_elapsed) {
            tracing::info!(path = %encoded.path(), encoded_bytes = encoded.bytes().len(), ?decode_mode,
                decode_ms = decode_elapsed.as_secs_f64() * 1_000.0,
                backend_ms = (start.elapsed() - decode_elapsed).as_secs_f64() * 1_000.0,
                "admitted sound resource");
        }
        if let Some(stolen) = playback.stolen()
            && let Some(index) = self
                .active_voices
                .iter()
                .position(|voice| voice.handle == stolen)
        {
            let stolen_voice = self.active_voices.remove(index);
            self.decoder.release(stolen_voice.sound);
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
