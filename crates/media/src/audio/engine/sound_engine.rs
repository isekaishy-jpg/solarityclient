//! SoundEntries-driven orchestration over media-owned backend resources.

mod fades;
mod loading;
mod parameters;
mod positioning;

pub use loading::{SoundLoadHandle, SoundLoadRequest};

use std::num::NonZeroU16;
use std::time::Duration;

use solarity_asset::{AssetPath, AssetStore};

use crate::audio::backend::{
    SoundBackend, SoundBackendError, SoundOutput, SoundOutputInfo, SoundSpatialPosition,
    SoundVoiceHandle, SoundVoiceState,
};
use crate::audio::cache::SoundCache;
use crate::audio::codec::SoundDecoder;
use crate::audio::selection::SoundVariationSelector;
use crate::audio::spatial::{ResolvedSpatialSound, SpatialSoundCatalog, SpatialSoundError};

use loading::PendingVoice;

use super::status::SoundEngineError;
use super::types::{
    SoundCategory, SoundChannel, SoundEngineSettings, SoundPlayRequest, SoundPlayback,
    SoundSoftwareChannelCount,
};
use super::{
    AdvancedSoundDucking, AdvancedSoundInstanceId, AdvancedSoundListener, AdvancedSoundSpatialMix,
    SoundFade,
};

/// Hard `maxchannels` argument passed to FMOD System::init by build 12340.
const STOCK_VIRTUAL_VOICE_CAPACITY: u16 = 512;

/// Policy retained for one voice while live CVar settings can change.
#[derive(Clone, Copy, Debug)]
struct ActiveVoice {
    handle: SoundVoiceHandle,
    sound: crate::audio::codec::DecodedSoundHandle,
    entry_id: Option<u32>,
    channel: SoundChannel,
    category: SoundCategory,
    source_gain: f32,
    runtime_gain: f32,
    spatial_gain: f32,
    fade: SoundFade,
    spatial_source: Option<positioning::PositionedSoundSource>,
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
    pending_voices: Vec<PendingVoice>,
    variation_selectors: Vec<(u32, SoundVariationSelector)>,
    listener: Option<AdvancedSoundListener>,
    background_muted: bool,
}

impl<'output> SoundEngine<'output> {
    /// Reopens the backend after native restart retirement, keeping sound-owner
    /// generations queryable as stopped and rejecting all late load completions.
    pub(super) fn restart_output(
        &mut self,
        output: &'output SoundOutput,
        software_channel_count: SoundSoftwareChannelCount,
    ) -> Result<(), SoundEngineError> {
        let count = NonZeroU16::new(software_channel_count.value())
            .ok_or(SoundBackendError::VoiceCapacity)?;
        let replacement = self.backend.restarted(output, count)?;
        replacement.set_background_muted(self.background_muted)?;
        for pending in self.pending_voices.drain(..) {
            if let Some(ticket) = pending.decode {
                self.decoder.cancel_load(ticket);
            }
        }
        self.backend = replacement;
        Ok(())
    }

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
            pending_voices: Vec::new(),
            listener: None,
            background_muted: false,
        })
    }

    /// Applies 4C5DC0's focus mute independently of Sound_EnableAllSound.
    ///
    /// Playback timelines, pending admission, and movie PCM remain active.
    /// 8794A0 and 87A8E0 set bus multipliers, not per-voice pause state.
    ///
    /// # Errors
    /// Returns a backend gain failure.
    pub fn set_background_muted(&mut self, muted: bool) -> Result<(), SoundEngineError> {
        if self.background_muted != muted {
            self.backend.set_background_muted(muted)?;
            self.background_muted = muted;
        }
        Ok(())
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

    /// Starts the dedicated non-spatial cinematic PCM stream.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when the backend cannot create or start it.
    pub fn start_cinematic_audio(
        &mut self,
        samples: &[i16],
        gain: f32,
    ) -> Result<(), SoundEngineError> {
        self.backend.start_cinematic_audio(samples, gain)?;
        Ok(())
    }

    /// Appends decoded samples to the active cinematic stream.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when no movie stream is active or the
    /// backend rejects the data.
    pub fn queue_cinematic_audio(&mut self, samples: &[i16]) -> Result<(), SoundEngineError> {
        self.backend.queue_cinematic_audio(samples)?;
        Ok(())
    }

    /// Returns the output-consumed position of the active movie stream.
    #[must_use]
    pub fn cinematic_playback_time(&self) -> Option<Duration> {
        self.backend.cinematic_playback_time()
    }

    /// Stops the dedicated cinematic stream if one is active.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when the backend cannot stop it.
    pub fn stop_cinematic_audio(&mut self) -> Result<(), SoundEngineError> {
        self.backend.stop_cinematic_audio()?;
        Ok(())
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

    /// Resolves the numeric-or-name namespace accepted by stock `PlaySound`.
    #[must_use]
    pub fn script_sound_entry_id(&self, name: &str) -> Option<u32> {
        name.parse::<u32>()
            .ok()
            .and_then(|id| self.catalog.sound_entry(id).map(|entry| entry.id()))
            .or_else(|| {
                self.catalog
                    .script_sound_entry(name)
                    .map(|entry| entry.id())
            })
    }

    /// Resolves named Glue music and ambience in `SoundEntries.dbc`.
    #[must_use]
    pub fn internal_sound_entry_id(&self, name: &str) -> Option<u32> {
        self.catalog
            .sound_entry_by_internal_name(name)
            .map(|entry| entry.id())
    }

    /// Returns the loop flag used when a retained effect admits an Entry-mode sound.
    #[must_use]
    pub fn sound_entry_loops(&self, entry_id: u32) -> bool {
        self.catalog
            .sound_entry(entry_id)
            .is_some_and(|entry| super::SoundLoopMode::Entry.is_looping(entry.flags()))
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
        let Some(load) = self.begin_load(request, next_random_word)? else {
            return Ok(SoundPlayback::Suppressed);
        };
        self.load_immediate(store, load)
    }

    /// Starts one exact archive path without inventing a `SoundEntries` row.
    ///
    /// Build 12340 routes `PlaySoundFile` through channel four and `PlayMusic`
    /// through channel five. The caller selects that exact channel and loop
    /// override; this boundary applies only the channel's stock gain policy.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for exact asset/decode failure, channel
    /// exhaustion, or backend admission failure.
    pub fn play_file(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        channel: SoundChannel,
        loop_mode: super::SoundLoopMode,
    ) -> Result<SoundPlayback, SoundEngineError> {
        let Some(load) = self.begin_file_load(path, channel, loop_mode)? else {
            return Ok(SoundPlayback::Suppressed);
        };
        self.load_immediate(store, load)
    }

    /// Selects and starts one ordinary voice at an exact world position.
    ///
    /// This is the `playSoundEntryAt` boundary used by model callbacks. It
    /// applies the base `SoundEntries` minimum/cutoff distances with a fully
    /// three-dimensional pan, but does not invent advanced-kit cone, schedule,
    /// influence, or ducking state for a row that has none.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] for the same exact playback failures as
    /// [`Self::play`], or when the emitter/base-row spatial values are invalid.
    pub fn play_positioned(
        &mut self,
        store: &mut AssetStore,
        request: SoundPlayRequest,
        listener: AdvancedSoundListener,
        emitter_position: glam::Vec3,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<SoundPlayback, SoundEngineError> {
        let Some(load) =
            self.begin_positioned_load(request, listener, emitter_position, next_random_word)?
        else {
            return Ok(SoundPlayback::Suppressed);
        };
        self.load_immediate(store, load)
    }

    /// Resolves ordinary spatial gain before a nonblocking payload load starts.
    ///
    /// # Errors
    /// Returns [`SoundEngineError`] for an absent entry or invalid spatial input.
    pub fn positioned_mix(
        &self,
        entry_id: u32,
        listener: AdvancedSoundListener,
        emitter_position: glam::Vec3,
    ) -> Result<AdvancedSoundSpatialMix, SoundEngineError> {
        let entry = self
            .catalog
            .sound_entry(entry_id)
            .ok_or(SoundEngineError::MissingEntry { entry_id })?;
        Ok(AdvancedSoundSpatialMix::evaluate_positioned(
            listener,
            emitter_position,
            entry,
        )?)
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
        if self.settings != settings {
            // Voice admission and runtime-gain changes already apply the current
            // policy. Reapplying an identical snapshot would repeatedly lock
            // the mixer and rebalance every voice during otherwise idle frames.
            for voice in &self.active_voices {
                let gain = applied_gain(settings, *voice);
                self.backend.set_gain(voice.handle, gain)?;
            }
            // Keep a failed update retryable even if some voices were updated.
            self.settings = settings;
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
        if voice.runtime_gain == runtime_gain {
            return Ok(());
        }
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
            if voice.duck_gain == duck_gain {
                continue;
            }
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

    /// Stops and retires every voice in one stock volume category.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when the backend cannot stop an owned
    /// voice.
    pub fn stop_category(&mut self, category: SoundCategory) -> Result<usize, SoundEngineError> {
        let pending_before = self.pending_voices.len();
        self.pending_voices.retain(|voice| {
            if voice.channel.category() != category {
                return true;
            }
            if let Some(ticket) = voice.decode {
                self.decoder.cancel_load(ticket);
            }
            false
        });
        let mut stopped = pending_before - self.pending_voices.len();
        let mut index = 0;
        while index < self.active_voices.len() {
            if self.active_voices[index].category != category {
                index += 1;
                continue;
            }
            self.backend.stop(self.active_voices[index].handle)?;
            let voice = self.active_voices.remove(index);
            self.decoder.release(voice.sound);
            stopped += 1;
        }
        Ok(stopped)
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
        * voice.spatial_gain
        * voice.fade.gain()
        * voice.duck_gain
}
