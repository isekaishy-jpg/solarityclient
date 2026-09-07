//! Retained world positions, updated whenever the shared listener changes.

use glam::Vec3;

use super::{
    AdvancedSoundListener, AdvancedSoundSpatialMix, SoundEngine, SoundEngineError,
    SoundVoiceHandle, applied_gain,
};

/// Authored spatial policy and fixed world origin retained for one voice.
#[derive(Clone, Copy, Debug)]
pub(super) struct PositionedSoundSource {
    pub(super) entry_id: u32,
    pub(super) position: Vec3,
}

impl SoundEngine<'_> {
    /// Tests the model-loop exclusion sphere from 0x004cfe00.
    ///
    /// Native 0x00879ae0 includes playing and paused channels, excludes pending
    /// loads, and leaves the reported position zero for non-3D voices.
    /// The strict squared-distance limit is 6.0, not a six-unit radius.
    ///
    /// # Errors
    /// Returns a backend voice-state query failure.
    pub fn has_nearby_entry(
        &self,
        entry_id: u32,
        position: Vec3,
    ) -> Result<bool, SoundEngineError> {
        for voice in &self.active_voices {
            if voice.entry_id != Some(entry_id) {
                continue;
            }
            let origin = match self.backend.state(voice.handle)? {
                crate::audio::backend::SoundVoiceState::Stopped => continue,
                crate::audio::backend::SoundVoiceState::Paused
                | crate::audio::backend::SoundVoiceState::Playing => voice
                    .spatial_source
                    .map_or(Vec3::ZERO, |source| source.position),
            };
            if origin.distance_squared(position) < 6.0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Retains an ordinary voice's world position independently of fade/duck gain.
    pub(super) fn position_voice(
        &mut self,
        handle: SoundVoiceHandle,
        entry_id: u32,
        position: Vec3,
        mix: AdvancedSoundSpatialMix,
    ) -> Result<(), SoundEngineError> {
        let index = self
            .active_voices
            .iter()
            .position(|voice| voice.handle == handle)
            .ok_or(SoundEngineError::UnknownVoice)?;
        let mut voice = self.active_voices[index];
        voice.spatial_source = Some(PositionedSoundSource { entry_id, position });
        voice.spatial_gain = mix.three_dimensional_gain();
        self.backend
            .set_gain(handle, applied_gain(self.settings, voice))?;
        self.backend
            .set_spatial_position(handle, mix.backend_position())?;
        self.active_voices[index] = voice;
        Ok(())
    }

    /// Positions a newly loaded SoundEntries voice using the current listener.
    ///
    /// The entry key is explicit so a direct-file voice cannot accidentally
    /// invent SoundEntries distances. The caller passes its selected entry key.
    ///
    /// # Errors
    /// Returns an unknown-voice, absent-entry, spatial-input, or backend error.
    pub fn set_voice_world_position(
        &mut self,
        handle: SoundVoiceHandle,
        entry_id: u32,
        listener: AdvancedSoundListener,
        position: Vec3,
    ) -> Result<(), SoundEngineError> {
        let mix = self.positioned_mix(entry_id, listener, position)?;
        self.position_voice(handle, entry_id, position, mix)
    }

    /// Repositions every retained ordinary voice after camera or player movement.
    ///
    /// SoundEntries positions remain in world space. Listener-relative backend
    /// positions must be recalculated; otherwise a source turns with the camera.
    /// Advanced emitters retain their separately owned cone and influence policy.
    ///
    /// # Errors
    /// Returns a spatial-input, absent-entry, or backend failure.
    pub fn update_listener(
        &mut self,
        listener: AdvancedSoundListener,
    ) -> Result<(), SoundEngineError> {
        if self.listener == Some(listener) {
            return Ok(());
        }
        self.collect_stopped_unmanaged_voices()?;
        for index in 0..self.active_voices.len() {
            let voice = self.active_voices[index];
            if let Some(source) = voice.spatial_source {
                let mix = self.positioned_mix(source.entry_id, listener, source.position)?;
                self.position_voice(voice.handle, source.entry_id, source.position, mix)?;
            }
        }
        self.listener = Some(listener);
        Ok(())
    }
}
