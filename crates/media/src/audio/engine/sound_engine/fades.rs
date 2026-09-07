//! Live mixer envelope updates, keeping resource retirement inside the engine.

use super::{
    SoundChannel, SoundEngine, SoundEngineError, SoundFade, SoundVoiceHandle, applied_gain,
};
use crate::audio::engine::SoundFadeDirection;
use std::time::Duration;

impl SoundEngine<'_> {
    /// Fades one exact channel, as `StopAllSFX` does to channel zero at 0x0087a920.
    /// Other SFX-volume channels (footsteps, armor, and vocals) remain independent.
    /// Pending resource requests are cancelled so completion cannot start a sound
    /// after the command has stopped its channel.
    ///
    /// # Errors
    /// Returns a backend gain failure for an admitted voice.
    pub fn fade_channel_out(
        &mut self,
        channel: SoundChannel,
        seconds: f32,
    ) -> Result<(), SoundEngineError> {
        self.pending_voices.retain(|voice| {
            if voice.channel != channel {
                return true;
            }
            if let Some(ticket) = voice.decode {
                self.decoder.cancel_load(ticket);
            }
            false
        });
        for index in 0..self.active_voices.len() {
            let voice = self.active_voices[index];
            if voice.channel == channel {
                let mut fade = voice.fade;
                fade.retarget_seconds(SoundFadeDirection::Out, seconds);
                self.set_voice_fade(voice.handle, fade)?;
            }
        }
        Ok(())
    }

    /// Reads an owned voice's envelope to reverse a transition without a jump.
    ///
    /// # Errors
    /// Returns [`SoundEngineError::UnknownVoice`] after generation retirement.
    pub fn voice_fade(&self, handle: SoundVoiceHandle) -> Result<SoundFade, SoundEngineError> {
        self.active_voices
            .iter()
            .find(|voice| voice.handle == handle)
            .map(|voice| voice.fade)
            .ok_or(SoundEngineError::UnknownVoice)
    }

    /// Applies an envelope independently of spatial, ducking, and CVar volume.
    ///
    /// # Errors
    /// Returns an unknown-voice or backend gain failure.
    pub fn set_voice_fade(
        &mut self,
        handle: SoundVoiceHandle,
        fade: SoundFade,
    ) -> Result<(), SoundEngineError> {
        let index = self
            .active_voices
            .iter()
            .position(|voice| voice.handle == handle)
            .ok_or(SoundEngineError::UnknownVoice)?;
        let mut voice = self.active_voices[index];
        voice.fade = fade;
        self.backend
            .set_gain(handle, applied_gain(self.settings, voice))?;
        self.active_voices[index] = voice;
        Ok(())
    }

    /// Advances all envelopes and retires completed fade-outs exactly once.
    ///
    /// # Errors
    /// Returns a backend gain or retirement failure.
    pub fn advance_fades(&mut self, elapsed: Duration) -> Result<(), SoundEngineError> {
        self.collect_stopped_unmanaged_voices()?;
        let mut index = 0;
        while index < self.active_voices.len() {
            let voice = self.active_voices[index];
            let mut fade = voice.fade;
            if fade.advance(elapsed) {
                self.stop(voice.handle)?;
            } else {
                if fade.gain() != voice.fade.gain() {
                    self.set_voice_fade(voice.handle, fade)?;
                } else {
                    self.active_voices[index].fade = fade;
                }
                index += 1;
            }
        }
        Ok(())
    }
}
