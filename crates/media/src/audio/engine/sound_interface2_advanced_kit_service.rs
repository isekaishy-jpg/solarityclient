//! Live advanced-sound instances composed with selection and backend voices.

use glam::Vec3;
use solarity_asset::AssetStore;
use thiserror::Error;

use crate::audio::backend::{SoundVoiceHandle, SoundVoiceState};
use crate::audio::selection::SoundVariationMode;
use crate::audio::spatial::SpatialSoundError;

use super::SoundLoopMode;
use super::sound_engine::SoundEngine;
use super::sound_interface2_advanced_kit_ducking::{AdvancedSoundDucking, AdvancedSoundInstanceId};
use super::sound_interface2_advanced_kit_lifecycle::{
    AdvancedSoundDirective, AdvancedSoundLifecycle, AdvancedSoundUsage, AdvancedSoundUsageError,
};
use super::sound_interface2_advanced_kit_properties::AdvancedSoundProperties;
use super::sound_interface2_advanced_kit_spatial::{
    AdvancedSoundListener, AdvancedSoundSpatialError, AdvancedSoundSpatialMix,
};
use super::status::{SoundCategoryError, SoundEngineError};
use super::types::{SoundCategory, SoundPlayRequest, SoundPlayback};

/// Authored identity and transform needed to create one instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedSoundCreateRequest {
    advanced_entry_id: u32,
    emitter_position: Vec3,
    cone_orientation: Vec3,
}

impl AdvancedSoundCreateRequest {
    /// Captures one MCSE-style advanced sound without deriving missing fields.
    #[must_use]
    pub const fn new(
        advanced_entry_id: u32,
        emitter_position: Vec3,
        cone_orientation: Vec3,
    ) -> Self {
        Self {
            advanced_entry_id,
            emitter_position,
            cone_orientation,
        }
    }
}

/// Process-wide owner of advanced instances and their shared ducking list.
#[derive(Debug, Default)]
pub struct AdvancedSoundService {
    next_instance_id: u64,
    instances: Vec<AdvancedSoundInstance>,
    ducking: AdvancedSoundDucking,
}

impl AdvancedSoundService {
    /// Creates an empty service with stock's first numeric instance identity.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_instance_id: 0,
            instances: Vec::new(),
            ducking: AdvancedSoundDucking::new(),
        }
    }

    /// Resolves and constructs one live advanced object.
    ///
    /// Schedule-offset and interval random words are consumed by the lifecycle
    /// in constructor order. Usage-two sounds then start immediately; their
    /// shared variation selector consumes another word only when the loaded
    /// base sound has multiple files.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundServiceError`] for exact row, category, lifecycle,
    /// spatial, asset, decode, backend, or identity failures.
    pub fn create(
        &mut self,
        store: &mut AssetStore,
        engine: &mut SoundEngine<'_>,
        request: AdvancedSoundCreateRequest,
        listener: AdvancedSoundListener,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<AdvancedSoundInstanceId, AdvancedSoundServiceError> {
        let (sound_entry_id, variation_count, properties, category) = {
            let resolved = engine.resolve_spatial_sound(request.advanced_entry_id)?;
            let properties = AdvancedSoundProperties::from(resolved.advanced_entry());
            let category = SoundCategory::from_volume_slider_category(
                resolved.advanced_entry().volume_slider_category(),
            )?;
            // Validate all FMOD-facing values before changing service state.
            AdvancedSoundSpatialMix::evaluate(
                listener,
                request.emitter_position,
                request.cone_orientation,
                properties,
                resolved.sound_entry(),
            )?;
            (
                resolved.sound_entry().id(),
                resolved.sound_entry().assets().len(),
                properties,
                category,
            )
        };
        let instance_id = AdvancedSoundInstanceId::new(self.next_instance_id);
        self.next_instance_id = self
            .next_instance_id
            .checked_add(1)
            .ok_or(AdvancedSoundServiceError::IdentityCapacity)?;
        let (lifecycle, initial_directive) =
            AdvancedSoundLifecycle::new(properties, next_random_word)?;
        self.instances.push(AdvancedSoundInstance {
            id: instance_id,
            advanced_entry_id: request.advanced_entry_id,
            sound_entry_id,
            variation_count,
            properties,
            category,
            emitter_position: request.emitter_position,
            cone_orientation: request.cone_orientation,
            lifecycle,
            voice: None,
        });

        if initial_directive == AdvancedSoundDirective::Play {
            let index = self.instances.len() - 1;
            if let Err(error) =
                play_instance(&mut self.instances[index], store, engine, next_random_word)
            {
                self.instances.pop();
                return Err(error);
            }
            self.configure_instance(index, engine, listener, 1.0)?;
            self.register_influence_if_inside(index, listener);
            self.ducking
                .update(0, listener.position().to_array(), |source| {
                    instance_position(&self.instances, source)
                });
            engine.apply_advanced_ducking(&self.ducking)?;
        }
        Ok(instance_id)
    }

    /// Advances lifecycle, schedule, spatial, and global ducking state.
    ///
    /// `day_milliseconds` comes directly from the runtime's authoritative
    /// runtime `RealmClock` boundary; media never reads local wall time or
    /// creates a fallback clock.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundServiceError`] for negative elapsed time or any
    /// exact catalog, spatial, asset, decoder, or backend failure.
    pub fn update(
        &mut self,
        store: &mut AssetStore,
        engine: &mut SoundEngine<'_>,
        elapsed_milliseconds: i32,
        day_milliseconds: u32,
        listener: AdvancedSoundListener,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<AdvancedSoundUpdateReport, AdvancedSoundServiceError> {
        if elapsed_milliseconds < 0 {
            return Err(AdvancedSoundServiceError::NegativeElapsed {
                elapsed_milliseconds,
            });
        }
        // Release every completed service-owned generation before any later
        // instance can reuse its backend slot during this update.
        for instance in &mut self.instances {
            let Some(voice) = instance.voice else {
                continue;
            };
            if !engine.owns_voice(voice) {
                instance.voice = None;
                continue;
            }
            if engine.voice_state(voice)? == SoundVoiceState::Stopped {
                engine.stop(voice)?;
                instance.voice = None;
            }
        }
        let mut report = AdvancedSoundUpdateReport::default();
        let mut index = 0;
        while index < self.instances.len() {
            let is_playing = match self.instances[index].voice {
                Some(voice) => match engine.voice_state(voice)? {
                    SoundVoiceState::Playing | SoundVoiceState::Paused => true,
                    SoundVoiceState::Stopped => {
                        engine.stop(voice)?;
                        self.instances[index].voice = None;
                        false
                    }
                },
                None => false,
            };
            let usage = self.instances[index].lifecycle.usage();
            let scheduled_gain = if usage == AdvancedSoundUsage::OneShot {
                Some(1.0)
            } else {
                self.instances[index].properties.scheduled_gain(
                    day_milliseconds,
                    self.instances[index].lifecycle.random_offset_milliseconds(),
                )
            };
            let variation_count = self.instances[index].variation_count;
            let directive = self.instances[index].lifecycle.update(
                elapsed_milliseconds,
                scheduled_gain.is_some(),
                is_playing,
                variation_count,
                next_random_word,
            );

            match directive {
                AdvancedSoundDirective::None => {}
                AdvancedSoundDirective::Play => {
                    if play_instance(&mut self.instances[index], store, engine, next_random_word)? {
                        report.started += 1;
                    }
                }
                AdvancedSoundDirective::Restart => {
                    stop_instance(&mut self.instances[index], engine)?;
                    if play_instance(&mut self.instances[index], store, engine, next_random_word)? {
                        report.restarted += 1;
                    }
                }
                AdvancedSoundDirective::Stop => {
                    stop_instance(&mut self.instances[index], engine)?;
                    report.stopped += 1;
                }
                AdvancedSoundDirective::Retire => {
                    stop_instance(&mut self.instances[index], engine)?;
                    let retired = self.instances.remove(index);
                    self.ducking.remove_source(retired.id);
                    report.retired += 1;
                    continue;
                }
            }

            if self.instances[index].voice.is_some() {
                self.configure_instance(index, engine, listener, scheduled_gain.unwrap_or(0.0))?;
                self.register_influence_if_inside(index, listener);
            }
            index += 1;
        }

        self.ducking.update(
            elapsed_milliseconds,
            listener.position().to_array(),
            |source| instance_position(&self.instances, source),
        );
        engine.apply_advanced_ducking(&self.ducking)?;
        report.active = self.instances.len();
        Ok(report)
    }

    /// Updates a live emitter transform without changing its authored policy.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundServiceError::UnknownInstance`] when absent, or a
    /// spatial error for non-finite vectors.
    pub fn set_transform(
        &mut self,
        instance_id: AdvancedSoundInstanceId,
        emitter_position: Vec3,
        cone_orientation: Vec3,
    ) -> Result<(), AdvancedSoundServiceError> {
        if !emitter_position.is_finite() || !cone_orientation.is_finite() {
            return Err(AdvancedSoundSpatialError::NonFiniteEmitter.into());
        }
        let instance = self
            .instances
            .iter_mut()
            .find(|instance| instance.id == instance_id)
            .ok_or(AdvancedSoundServiceError::UnknownInstance { instance_id })?;
        instance.emitter_position = emitter_position;
        instance.cone_orientation = cone_orientation;
        Ok(())
    }

    /// Requests stock-style retirement during the next update.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundServiceError::UnknownInstance`] when absent.
    pub fn request_retire(
        &mut self,
        instance_id: AdvancedSoundInstanceId,
    ) -> Result<(), AdvancedSoundServiceError> {
        let instance = self
            .instances
            .iter_mut()
            .find(|instance| instance.id == instance_id)
            .ok_or(AdvancedSoundServiceError::UnknownInstance { instance_id })?;
        instance.lifecycle.request_retire();
        Ok(())
    }

    /// Stops and destroys every instance at a world-residency boundary.
    ///
    /// # Errors
    ///
    /// Returns [`AdvancedSoundServiceError`] when an owned backend voice cannot
    /// be stopped or the cleared ducking state cannot be applied.
    pub fn clear(
        &mut self,
        engine: &mut SoundEngine<'_>,
    ) -> Result<usize, AdvancedSoundServiceError> {
        for instance in &mut self.instances {
            stop_instance(instance, engine)?;
        }
        let cleared = self.instances.len();
        self.instances.clear();
        self.ducking = AdvancedSoundDucking::new();
        engine.apply_advanced_ducking(&self.ducking)?;
        Ok(cleared)
    }

    /// Returns the number of constructed advanced objects not yet retired.
    #[must_use]
    pub fn active_instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Returns one instance's current engine voice, if playing or retained.
    #[must_use]
    pub fn voice(&self, instance_id: AdvancedSoundInstanceId) -> Option<SoundVoiceHandle> {
        self.instances
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| instance.voice)
    }

    /// Resolves and applies current spatial and schedule policy to one voice.
    fn configure_instance(
        &self,
        index: usize,
        engine: &mut SoundEngine<'_>,
        listener: AdvancedSoundListener,
        scheduled_gain: f32,
    ) -> Result<(), AdvancedSoundServiceError> {
        let instance = &self.instances[index];
        let Some(voice) = instance.voice else {
            return Ok(());
        };
        let mix = {
            let resolved = engine.resolve_spatial_sound(instance.advanced_entry_id)?;
            AdvancedSoundSpatialMix::evaluate(
                listener,
                instance.emitter_position,
                instance.cone_orientation,
                instance.properties,
                resolved.sound_entry(),
            )?
        };
        engine.set_voice_runtime_gain(voice, scheduled_gain * mix.three_dimensional_gain())?;
        engine.set_voice_spatial_mix(voice, mix.backend_position(), mix.pan_level())?;
        Ok(())
    }

    /// Registers a playing source at the stock inner influence boundary.
    fn register_influence_if_inside(&mut self, index: usize, listener: AdvancedSoundListener) {
        let instance = &self.instances[index];
        if instance.voice.is_none() {
            return;
        }
        let inner_radius = instance.properties.influence_radii()[0];
        let displacement = listener.position() - instance.emitter_position;
        if instance.emitter_position == Vec3::ZERO
            || displacement.length_squared() <= inner_radius * inner_radius
        {
            self.ducking.register(instance.id, instance.properties);
        }
    }
}

/// Counts backend operations completed during one service update.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdvancedSoundUpdateReport {
    /// Voices started from idle lifecycle state.
    pub started: usize,
    /// Continuous voices replaced with another variation.
    pub restarted: usize,
    /// Voices stopped at an inactive schedule boundary.
    pub stopped: usize,
    /// Terminal advanced objects destroyed.
    pub retired: usize,
    /// Advanced objects remaining after the update.
    pub active: usize,
}

/// Exact failure while constructing or updating advanced sound instances.
#[derive(Debug, Error)]
pub enum AdvancedSoundServiceError {
    /// The monotonic instance identifier exhausted 64-bit storage.
    #[error("advanced sound instance identity capacity is exhausted")]
    IdentityCapacity,
    /// A caller named an instance that has already retired or never existed.
    #[error("advanced sound instance {} is absent", instance_id.value())]
    UnknownInstance {
        /// Exact requested identity.
        instance_id: AdvancedSoundInstanceId,
    },
    /// Runtime supplied a backwards sound-engine duration.
    #[error("advanced sound elapsed milliseconds must be nonnegative, got {elapsed_milliseconds}")]
    NegativeElapsed {
        /// Unmodified elapsed value.
        elapsed_milliseconds: i32,
    },
    /// Exact advanced/base DBC join failed.
    #[error(transparent)]
    Catalog(#[from] SpatialSoundError),
    /// Advanced usage has no stock behavior.
    #[error(transparent)]
    Usage(#[from] AdvancedSoundUsageError),
    /// Advanced volume-slider word is outside the stock channel table.
    #[error(transparent)]
    Category(#[from] SoundCategoryError),
    /// Listener, emitter, distance, or cone data is invalid.
    #[error(transparent)]
    Spatial(#[from] AdvancedSoundSpatialError),
    /// Exact sound selection, asset, decode, or backend operation failed.
    #[error(transparent)]
    Engine(#[from] SoundEngineError),
}

/// Complete mutable state for one advanced sound object.
#[derive(Debug)]
struct AdvancedSoundInstance {
    id: AdvancedSoundInstanceId,
    advanced_entry_id: u32,
    sound_entry_id: u32,
    variation_count: usize,
    properties: AdvancedSoundProperties,
    category: SoundCategory,
    emitter_position: Vec3,
    cone_orientation: Vec3,
    lifecycle: AdvancedSoundLifecycle,
    voice: Option<SoundVoiceHandle>,
}

/// Starts one selected backend voice and records suppression explicitly.
fn play_instance(
    instance: &mut AdvancedSoundInstance,
    store: &mut AssetStore,
    engine: &mut SoundEngine<'_>,
    next_random_word: &mut impl FnMut() -> u32,
) -> Result<bool, AdvancedSoundServiceError> {
    let request = SoundPlayRequest::advanced(
        instance.sound_entry_id,
        instance.category,
        SoundVariationMode::Random,
        if instance.lifecycle.usage() == AdvancedSoundUsage::Continuous {
            SoundLoopMode::Loop
        } else {
            SoundLoopMode::Once
        },
        instance.id,
    );
    match engine.play(store, request, next_random_word)? {
        SoundPlayback::Started(voice) => {
            instance.voice = Some(voice);
            Ok(true)
        }
        SoundPlayback::Suppressed => {
            instance.voice = None;
            Ok(false)
        }
    }
}

/// Stops and clears one retained voice generation when present.
fn stop_instance(
    instance: &mut AdvancedSoundInstance,
    engine: &mut SoundEngine<'_>,
) -> Result<(), AdvancedSoundServiceError> {
    if let Some(voice) = instance.voice.take()
        && engine.owns_voice(voice)
    {
        engine.stop(voice)?;
    }
    Ok(())
}

/// Finds the current transform used by the shared ducking list.
fn instance_position(
    instances: &[AdvancedSoundInstance],
    source: AdvancedSoundInstanceId,
) -> Option<[f32; 3]> {
    instances
        .iter()
        .find(|instance| instance.id == source)
        .map(|instance| instance.emitter_position.to_array())
}
