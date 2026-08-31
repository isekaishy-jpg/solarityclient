//! SoundEntries-driven orchestration over media-owned backend resources.

use std::num::NonZeroU16;

use solarity_asset::AssetStore;

use crate::audio::backend::{
    SoundBackend, SoundBackendError, SoundOutput, SoundOutputInfo, SoundSpatialPosition,
    SoundVoiceHandle, SoundVoiceState,
};
use crate::audio::cache::SoundCache;
use crate::audio::codec::SoundDecoder;
use crate::audio::selection::SoundVariationSelector;
use crate::audio::spatial::{ResolvedSpatialSound, SpatialSoundCatalog, SpatialSoundError};

use super::status::SoundEngineError;
use super::types::{
    SoundCategory, SoundChannel, SoundEngineSettings, SoundPlayRequest, SoundPlayback,
    SoundSoftwareChannelCount,
};
use super::{AdvancedSoundDucking, AdvancedSoundInstanceId};

/// Hard `maxchannels` argument passed to FMOD System::init by build 12340.
const STOCK_VIRTUAL_VOICE_CAPACITY: u16 = 512;

/// Policy retained for one voice while live CVar settings can change.
#[derive(Clone, Copy, Debug)]
struct ActiveVoice {
    handle: SoundVoiceHandle,
    sound: crate::audio::codec::DecodedSoundHandle,
    entry_id: u32,
    channel: SoundChannel,
    category: SoundCategory,
    source_gain: f32,
    runtime_gain: f32,
    duck_gain: f32,
    duck_source: Option<AdvancedSoundInstanceId>,
}

/// Stock-facing sound selection, admission, and live-volume owner.
pub struct SoundEngine<'output> {
    catalog: SpatialSoundCatalog,
    cache: SoundCache,
    // Tracks must drop before their referenced SDL Audio resources.
    backend: SoundBackend<'output>,
    decoder: SoundDecoder,
    settings: SoundEngineSettings,
    active_voices: Vec<ActiveVoice>,
    variation_selectors: Vec<(u32, SoundVariationSelector)>,
}

impl<'output> SoundEngine<'output> {
    /// Loads the stock sound catalog and allocates the exact virtual pool.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for exact DBC/archive failures, decoder
    /// initialization failure, or backend track-allocation failure.
    pub fn load(
        store: &mut AssetStore,
        output: &'output SoundOutput,
        software_channel_count: SoundSoftwareChannelCount,
        settings: SoundEngineSettings,
    ) -> Result<Self, SoundEngineError> {
        let catalog = SpatialSoundCatalog::load(store)?;
        let decoder = SoundDecoder::new()?;
        let Some(virtual_voice_capacity) = NonZeroU16::new(STOCK_VIRTUAL_VOICE_CAPACITY) else {
            return Err(SoundBackendError::VoiceCapacity.into());
        };
        let Some(software_channel_count) = NonZeroU16::new(software_channel_count.value()) else {
            return Err(SoundBackendError::VoiceCapacity.into());
        };
        let backend = SoundBackend::new(output, software_channel_count, virtual_voice_capacity)?;
        Ok(Self {
            catalog,
            cache: SoundCache::new(),
            backend,
            decoder,
            settings,
            active_voices: Vec::with_capacity(usize::from(STOCK_VIRTUAL_VOICE_CAPACITY)),
            variation_selectors: Vec::new(),
        })
    }

    /// Returns the currently applied global and category policy.
    #[must_use]
    pub const fn settings(&self) -> SoundEngineSettings {
        self.settings
    }

    /// Returns the selected output target and actual SDL mixer format.
    #[must_use]
    pub const fn output_info(&self) -> SoundOutputInfo {
        self.backend.output_info()
    }

    /// Returns the executable's hard maximum number of virtual voices.
    #[must_use]
    pub fn voice_capacity(&self) -> usize {
        self.backend.voice_capacity()
    }

    /// Returns the real software-mix count sourced from `Sound_NumChannels`.
    #[must_use]
    pub const fn software_channel_count(&self) -> usize {
        self.backend.software_channel_count()
    }

    /// Returns the number of voices that have not yet been collected.
    #[must_use]
    pub fn active_voice_count(&self) -> usize {
        self.active_voices.len()
    }

    /// Returns the number of normalized encoded paths retained by the cache.
    #[must_use]
    pub fn cached_sound_count(&self) -> usize {
        self.cache.len()
    }

    /// Returns the number of path/mode decoder resources admitted so far.
    #[must_use]
    pub fn decoded_sound_count(&self) -> usize {
        self.decoder.len()
    }

    /// Returns encoded-byte accounting for cached predecoded samples.
    #[must_use]
    pub const fn decoded_sample_cache_bytes(&self) -> usize {
        self.decoder.cached_sample_bytes()
    }

    /// Resolves one terrain/advanced identifier to its exact authored rows.
    ///
    /// This performs no attenuation, cone, timing, or ducking interpretation.
    ///
    /// # Errors
    ///
    /// Returns [`SpatialSoundError`] when either exact authored key is absent.
    pub fn resolve_spatial_sound(
        &self,
        advanced_entry_id: u32,
    ) -> Result<ResolvedSpatialSound<'_>, SpatialSoundError> {
        self.catalog.resolve(advanced_entry_id)
    }

    /// Selects and starts one sound through its stock shared variation state.
    ///
    /// Disabled global/category policy returns [`SoundPlayback::Suppressed`]
    /// before consuming a random word, selecting a file, or reading its
    /// payload. No neighboring entry, path, codec, or voice is substituted
    /// after any failure.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for an unknown or unselectable definition,
    /// invalid authored volume, exact asset/decode failure, or backend
    /// exhaustion.
    pub fn play(
        &mut self,
        store: &mut AssetStore,
        request: SoundPlayRequest,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<SoundPlayback, SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        let entry =
            self.catalog
                .sound_entry(request.entry_id())
                .ok_or(SoundEngineError::MissingEntry {
                    entry_id: request.entry_id(),
                })?;
        let channel = request.channel();
        let category = channel.category();
        let Some(category_gain) = self.settings.category_gain(category) else {
            return Ok(SoundPlayback::Suppressed);
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
                >= maximum
        {
            return Err(SoundEngineError::ChannelCapacity {
                channel: channel.value(),
                maximum,
            });
        }
        if request.concurrency_mode().is_exclusive(entry.flags())
            && self
                .active_voices
                .iter()
                .any(|voice| voice.entry_id == entry.id())
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
        let encoded = self.cache.load(store, &asset_path)?;
        let decode_mode = self
            .settings
            .residency()
            .decode_mode(encoded.path(), encoded.bytes().len());
        let sound = self.decoder.load(&encoded, decode_mode)?;
        let source_gain = entry.volume();
        let playback = match self.backend.play(
            &self.decoder,
            sound,
            category_gain * source_gain,
            request.loop_mode().is_looping(entry.flags()),
            request.priority(),
        ) {
            Ok(playback) => playback,
            Err(error) => {
                self.decoder.release(sound);
                return Err(error.into());
            }
        };
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
            entry_id: entry.id(),
            channel,
            category,
            source_gain,
            runtime_gain: 1.0,
            duck_gain: 1.0,
            duck_source: request.advanced_source(),
        });
        Ok(SoundPlayback::Started(voice))
    }

    /// Applies one validated CVar snapshot to every retained active voice.
    ///
    /// Disabled voices remain positioned and advance normally at zero gain, so
    /// later re-enablement does not invent a playback restart.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when backend state or gain application
    /// fails.
    pub fn set_settings(&mut self, settings: SoundEngineSettings) -> Result<(), SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        self.settings = settings;
        for voice in &self.active_voices {
            let gain = applied_gain(settings, *voice);
            self.backend.set_gain(voice.handle, gain)?;
        }
        self.decoder
            .trim_predecoded_cache(settings.residency().maximum_sample_cache_size_bytes() as usize);
        Ok(())
    }

    /// Applies one live schedule, spatial, and ducking multiplier to a voice.
    ///
    /// The multiplier is retained separately from source and CVar gains, so a
    /// later [`Self::set_settings`] call cannot erase current advanced-sound
    /// policy. Values above one remain representable because the backend and
    /// stock authored source path both permit amplification.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError::UnknownVoice`] for an unowned handle, or a
    /// backend error for a negative, non-finite, or unrepresentable gain.
    pub fn set_voice_runtime_gain(
        &mut self,
        handle: SoundVoiceHandle,
        runtime_gain: f32,
    ) -> Result<(), SoundEngineError> {
        if !runtime_gain.is_finite() || runtime_gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain: runtime_gain }.into());
        }
        let index = self
            .active_voices
            .iter()
            .position(|voice| voice.handle == handle)
            .ok_or(SoundEngineError::UnknownVoice)?;
        let mut voice = self.active_voices[index];
        voice.runtime_gain = runtime_gain;
        self.backend
            .set_gain(handle, applied_gain(self.settings, voice))?;
        self.active_voices[index].runtime_gain = runtime_gain;
        Ok(())
    }

    /// Applies or clears a listener-relative backend position for one voice.
    ///
    /// The caller supplies the position after stock world-to-listener, pan,
    /// distance, and cone policy has been evaluated. This boundary performs no
    /// coordinate fallback or policy inference.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError::UnknownVoice`] when this engine does not own
    /// the handle, or a backend error when SDL rejects the operation.
    pub fn set_voice_spatial_position(
        &self,
        handle: SoundVoiceHandle,
        position: Option<SoundSpatialPosition>,
    ) -> Result<(), SoundEngineError> {
        if !self
            .active_voices
            .iter()
            .any(|voice| voice.handle == handle)
        {
            return Err(SoundEngineError::UnknownVoice);
        }
        self.backend.set_spatial_position(handle, position)?;
        Ok(())
    }

    /// Applies an advanced voice's listener position and stock pan level.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError::UnknownVoice`] when this engine does not own
    /// the handle, or a backend error for invalid pan policy or SDL failure.
    pub(super) fn set_voice_spatial_mix(
        &self,
        handle: SoundVoiceHandle,
        position: Option<SoundSpatialPosition>,
        pan_level: f32,
    ) -> Result<(), SoundEngineError> {
        if !self
            .active_voices
            .iter()
            .any(|voice| voice.handle == handle)
        {
            return Err(SoundEngineError::UnknownVoice);
        }
        self.backend.set_spatial_mix(handle, position, pan_level)?;
        Ok(())
    }

    /// Applies the process-wide advanced influence list to every live voice.
    ///
    /// Advanced voices exclude their own attached influence. Ordinary voices
    /// have no exclusion and receive the minimum gain for their category.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when stopped-voice collection or a backend
    /// gain update fails.
    pub(super) fn apply_advanced_ducking(
        &mut self,
        ducking: &AdvancedSoundDucking,
    ) -> Result<(), SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        for voice in &mut self.active_voices {
            let duck_gain = ducking.category_gain(voice.category, voice.duck_source);
            let mut updated = *voice;
            updated.duck_gain = duck_gain;
            self.backend
                .set_gain(voice.handle, applied_gain(self.settings, updated))?;
            voice.duck_gain = duck_gain;
        }
        Ok(())
    }

    /// Returns one engine-owned voice's current backend state.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError::UnknownVoice`] when this engine does not own
    /// the handle, or a backend error when state inspection fails.
    pub fn voice_state(
        &self,
        handle: SoundVoiceHandle,
    ) -> Result<SoundVoiceState, SoundEngineError> {
        if !self
            .active_voices
            .iter()
            .any(|voice| voice.handle == handle)
        {
            return Err(SoundEngineError::UnknownVoice);
        }
        Ok(self.backend.state(handle)?)
    }

    /// Reports whether this exact generation remains in the engine registry.
    pub(super) fn owns_voice(&self, handle: SoundVoiceHandle) -> bool {
        self.active_voices
            .iter()
            .any(|voice| voice.handle == handle)
    }

    /// Stops and retires one engine-owned voice immediately.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError::UnknownVoice`] when this engine does not own
    /// the handle, or a backend error when stop fails.
    pub fn stop(&mut self, handle: SoundVoiceHandle) -> Result<(), SoundEngineError> {
        let index = self
            .active_voices
            .iter()
            .position(|voice| voice.handle == handle)
            .ok_or(SoundEngineError::UnknownVoice)?;
        self.backend.stop(handle)?;
        let voice = self.active_voices.remove(index);
        self.decoder.release(voice.sound);
        Ok(())
    }

    /// Removes naturally stopped voices without changing playing or paused ones.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] if the backend cannot inspect an owned
    /// voice.
    pub fn collect_stopped_voices(&mut self) -> Result<usize, SoundEngineError> {
        let before = self.active_voices.len();
        let mut index = 0;
        while index < self.active_voices.len() {
            if self.backend.state(self.active_voices[index].handle)? == SoundVoiceState::Stopped {
                self.backend.stop(self.active_voices[index].handle)?;
                let voice = self.active_voices.remove(index);
                self.decoder.release(voice.sound);
            } else {
                index += 1;
            }
        }
        Ok(before - self.active_voices.len())
    }

    /// Collects ordinary voices while service-owned generations remain stable.
    fn collect_stopped_unmanaged_voices(&mut self) -> Result<usize, SoundEngineError> {
        let before = self.active_voices.len();
        let mut index = 0;
        while index < self.active_voices.len() {
            let voice = self.active_voices[index];
            if voice.duck_source.is_none()
                && self.backend.state(voice.handle)? == SoundVoiceState::Stopped
            {
                self.backend.stop(voice.handle)?;
                let voice = self.active_voices.remove(index);
                self.decoder.release(voice.sound);
            } else {
                index += 1;
            }
        }
        Ok(before - self.active_voices.len())
    }

    /// Releases encoded payloads no longer held outside the cache.
    ///
    /// Decoder resources remain independently owned by SDL after their one
    /// adapter-boundary copy.
    pub fn collect_unused_encoded(&mut self) -> usize {
        self.cache.collect_unused()
    }

    /// Pulls mixed bytes from an explicit memory output for validation/tools.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] if this engine uses a device output or SDL
    /// reports memory-generation failure.
    pub fn generate(&self, buffer: &mut [u8]) -> Result<usize, SoundEngineError> {
        Ok(self.backend.generate(buffer)?)
    }
}

/// Composes independent source, live CVar, and runtime policy exactly once.
fn applied_gain(settings: SoundEngineSettings, voice: ActiveVoice) -> f32 {
    settings.category_gain(voice.category).unwrap_or(0.0)
        * voice.source_gain
        * voice.runtime_gain
        * voice.duck_gain
}
