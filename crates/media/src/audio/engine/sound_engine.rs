//! SoundEntries-driven orchestration over media-owned backend resources.

use std::num::NonZeroU16;

use solarity_asset::AssetStore;

use crate::audio::backend::{
    SoundBackend, SoundBackendError, SoundOutput, SoundSpatialPosition, SoundVoiceHandle,
    SoundVoiceState,
};
use crate::audio::cache::SoundCache;
use crate::audio::codec::SoundDecoder;
use crate::audio::selection::SoundVariationSelector;
use crate::audio::spatial::{ResolvedSpatialSound, SpatialSoundCatalog, SpatialSoundError};

use super::status::SoundEngineError;
use super::types::{SoundCategory, SoundEngineSettings, SoundPlayRequest, SoundPlayback};

/// Policy retained for one voice while live CVar settings can change.
#[derive(Clone, Copy, Debug)]
struct ActiveVoice {
    handle: SoundVoiceHandle,
    category: SoundCategory,
    source_gain: f32,
    runtime_gain: f32,
}

/// Stock-facing sound selection, admission, and live-volume owner.
pub struct SoundEngine<'output> {
    catalog: SpatialSoundCatalog,
    cache: SoundCache,
    decoder: SoundDecoder,
    backend: SoundBackend<'output>,
    settings: SoundEngineSettings,
    active_voices: Vec<ActiveVoice>,
    variation_selectors: Vec<(u32, SoundVariationSelector)>,
}

impl<'output> SoundEngine<'output> {
    /// Loads the stock sound catalog and allocates the explicit backend pool.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for exact DBC/archive failures, decoder
    /// initialization failure, or backend track-allocation failure.
    pub fn load(
        store: &mut AssetStore,
        output: &'output SoundOutput,
        voice_capacity: NonZeroU16,
        settings: SoundEngineSettings,
    ) -> Result<Self, SoundEngineError> {
        let catalog = SpatialSoundCatalog::load(store)?;
        let decoder = SoundDecoder::new()?;
        let backend = SoundBackend::new(output, voice_capacity)?;
        Ok(Self {
            catalog,
            cache: SoundCache::new(),
            decoder,
            backend,
            settings,
            active_voices: Vec::with_capacity(usize::from(voice_capacity.get())),
            variation_selectors: Vec::new(),
        })
    }

    /// Returns the currently applied global and category policy.
    #[must_use]
    pub const fn settings(&self) -> SoundEngineSettings {
        self.settings
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
        self.collect_stopped_voices()?;
        let entry =
            self.catalog
                .sound_entry(request.entry_id())
                .ok_or(SoundEngineError::MissingEntry {
                    entry_id: request.entry_id(),
                })?;
        let Some(category_gain) = self.settings.category_gain(request.category()) else {
            return Ok(SoundPlayback::Suppressed);
        };
        if !entry.volume().is_finite() || entry.volume() < 0.0 {
            return Err(SoundEngineError::InvalidEntryVolume {
                entry_id: entry.id(),
                volume: entry.volume(),
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
        let sound = self.decoder.load(&encoded, request.decode_mode())?;
        let source_gain = entry.volume();
        let voice = self.backend.play(
            &self.decoder,
            sound,
            category_gain * source_gain,
            request.looping(),
        )?;
        self.active_voices.push(ActiveVoice {
            handle: voice,
            category: request.category(),
            source_gain,
            runtime_gain: 1.0,
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
        self.collect_stopped_voices()?;
        self.settings = settings;
        for voice in &self.active_voices {
            let gain = applied_gain(settings, *voice);
            self.backend.set_gain(voice.handle, gain)?;
        }
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
        self.active_voices.remove(index);
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
                self.active_voices.remove(index);
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
    settings.category_gain(voice.category).unwrap_or(0.0) * voice.source_gain * voice.runtime_gain
}
