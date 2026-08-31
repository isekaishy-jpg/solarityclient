//! SDL3_mixer adapter replacing the stock client's FMOD output boundary.

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};

use sdl3::mixer::{Mixer, Point3D, Track};
use sdl3::sys::audio::{SDL_AUDIO_S16LE, SDL_AudioSpec};

use crate::audio::codec::{DecodedSoundHandle, SoundDecoder};

use super::status::SoundBackendError;
use super::types::{
    SoundOutputInfo, SoundOutputTarget, SoundSpatialPosition, SoundVoiceHandle, SoundVoiceState,
};

const STOCK_OUTPUT_SAMPLE_RATE_HZ: i32 = 44_100;
const STOCK_OUTPUT_CHANNEL_COUNT: i32 = 2;

/// One reusable SDL track and the generation currently exposed to callers.
struct VoiceSlot<'output> {
    track: Track<'output>,
    generation: u32,
}

/// Owner of one explicitly selected SDL output mixer.
///
/// A [`SoundBackend`] borrows this value, making SDL's required track-before-
/// mixer destruction order a normal Rust lifetime instead of an unsafe
/// self-reference.
pub struct SoundOutput {
    target: SoundOutputTarget,
    info: SoundOutputInfo,
    mixer: Mixer,
}

impl SoundOutput {
    /// Opens the selected output using build 12340's default format request.
    ///
    /// A device may choose another actual format, which is reported by
    /// [`Self::info`]. `Memory` remains an explicit validation/tooling target
    /// and is never a fallback for `DefaultDevice` failure.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] when SDL cannot create or inspect the
    /// requested output.
    pub fn open(target: SoundOutputTarget) -> Result<Self, SoundBackendError> {
        let requested_format = SDL_AudioSpec {
            format: SDL_AUDIO_S16LE,
            channels: STOCK_OUTPUT_CHANNEL_COUNT,
            freq: STOCK_OUTPUT_SAMPLE_RATE_HZ,
        };
        let mixer = match target {
            SoundOutputTarget::DefaultDevice => Mixer::open_device(Some(&requested_format)),
            SoundOutputTarget::Memory => Mixer::create_memory(Some(&requested_format)),
        }
        .map_err(|source| SoundBackendError::adapter("open sound output", source))?;
        let actual_format = mixer
            .format()
            .map_err(|source| SoundBackendError::adapter("inspect sound output format", source))?;
        let invalid_format = || SoundBackendError::InvalidOutputFormat {
            sample_rate_hz: actual_format.freq,
            channel_count: actual_format.channels,
        };
        let sample_rate_hz =
            u32::try_from(actual_format.freq).map_err(|_source| invalid_format())?;
        let channel_count =
            u8::try_from(actual_format.channels).map_err(|_source| invalid_format())?;
        if sample_rate_hz == 0 || channel_count == 0 {
            return Err(invalid_format());
        }
        Ok(Self {
            target,
            info: SoundOutputInfo::new(target, sample_rate_hz, channel_count),
            mixer,
        })
    }

    /// Returns the explicit target and actual mixer format.
    #[must_use]
    pub const fn info(&self) -> SoundOutputInfo {
        self.info
    }

    /// Pulls mixed bytes from an explicitly opened memory output.
    ///
    /// # Errors
    ///
    /// Returns the number of non-silence bytes mixed. SDL initializes the rest
    /// of the requested buffer with silence.
    ///
    /// Returns [`SoundBackendError`] when this output targets a device or SDL
    /// reports generation failure.
    pub fn generate(&self, buffer: &mut [u8]) -> Result<usize, SoundBackendError> {
        if self.target != SoundOutputTarget::Memory {
            return Err(SoundBackendError::NotMemoryOutput);
        }
        let generated = self.mixer.generate(buffer);
        if generated >= 0 {
            Ok(generated as usize)
        } else {
            Err(SoundBackendError::adapter(
                "generate sound memory output",
                sdl3::get_error(),
            ))
        }
    }
}

/// Fixed-capacity owner of reusable tracks on one borrowed output.
///
/// Capacity is always explicit because the stock `Sound_NumChannels` CVar is
/// the policy authority. The backend never creates an extra voice, steals one,
/// or changes output target after a failure.
pub struct SoundBackend<'output> {
    backend_id: u64,
    output: &'output SoundOutput,
    voices: Vec<VoiceSlot<'output>>,
}

impl<'output> SoundBackend<'output> {
    /// Allocates the complete voice pool on an already opened output.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] when SDL cannot create every explicitly
    /// requested track.
    pub fn new(
        output: &'output SoundOutput,
        voice_capacity: NonZeroU16,
    ) -> Result<Self, SoundBackendError> {
        static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(1);

        let mut voices = Vec::with_capacity(usize::from(voice_capacity.get()));
        for _slot in 0..voice_capacity.get() {
            let track = output
                .mixer
                .create_track()
                .map_err(|source| SoundBackendError::adapter("create sound voice", source))?;
            voices.push(VoiceSlot {
                track,
                generation: 0,
            });
        }
        Ok(Self {
            backend_id: NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed),
            output,
            voices,
        })
    }

    /// Returns the explicit target and actual mixer format.
    #[must_use]
    pub const fn output_info(&self) -> SoundOutputInfo {
        self.output.info
    }

    /// Returns the exact number of reusable voices allocated at open.
    #[must_use]
    pub fn voice_capacity(&self) -> usize {
        self.voices.len()
    }

    /// Starts one decoded sound on the first stopped track.
    ///
    /// A gain above one is retained because SDL and stock source-volume policy
    /// both permit amplification. Only negative and non-finite gains are
    /// rejected at this low-level boundary.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a foreign sound, invalid gain,
    /// exhausted pool, generation overflow, or SDL playback failure.
    pub fn play(
        &mut self,
        decoder: &SoundDecoder,
        sound: DecodedSoundHandle,
        gain: f32,
        looping: bool,
    ) -> Result<SoundVoiceHandle, SoundBackendError> {
        if !gain.is_finite() || gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain });
        }
        let audio = decoder
            .audio(sound)
            .ok_or(SoundBackendError::UnknownSound)?;
        let (slot_index, slot) = self
            .voices
            .iter_mut()
            .enumerate()
            .find(|(_index, slot)| !slot.track.is_playing() && !slot.track.is_paused())
            .ok_or(SoundBackendError::VoiceCapacity)?;
        let generation = slot
            .generation
            .checked_add(1)
            .ok_or(SoundBackendError::GenerationCapacity)?;

        if let Err(source) = slot.track.set_audio(audio) {
            return Err(SoundBackendError::adapter(
                "assign sound voice input",
                source,
            ));
        }
        let result = slot
            .track
            .set_loops(if looping { -1 } else { 0 })
            .and_then(|()| slot.track.set_gain(gain))
            .and_then(|()| slot.track.play());
        if let Err(source) = result {
            let _cleanup_result = slot.track.clear_audio();
            return Err(SoundBackendError::adapter("start sound voice", source));
        }
        slot.generation = generation;
        Ok(SoundVoiceHandle {
            backend_id: self.backend_id,
            slot: slot_index as u16,
            generation,
        })
    }

    /// Returns the current state of one live voice generation.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError::UnknownVoice`] for a stale or foreign
    /// handle.
    pub fn state(&self, voice: SoundVoiceHandle) -> Result<SoundVoiceState, SoundBackendError> {
        let slot = self.voice(voice)?;
        if slot.track.is_paused() {
            Ok(SoundVoiceState::Paused)
        } else if slot.track.is_playing() {
            Ok(SoundVoiceState::Playing)
        } else {
            Ok(SoundVoiceState::Stopped)
        }
    }

    /// Pauses one live voice without changing its playback position.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn pause(&self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        self.voice(voice)?
            .track
            .pause()
            .map_err(|source| SoundBackendError::adapter("pause sound voice", source))
    }

    /// Resumes one paused live voice.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn resume(&self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        self.voice(voice)?
            .track
            .resume()
            .map_err(|source| SoundBackendError::adapter("resume sound voice", source))
    }

    /// Changes one live voice's nonnegative gain without clamping it.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for an invalid gain, stale handle, or SDL
    /// failure.
    pub fn set_gain(&self, voice: SoundVoiceHandle, gain: f32) -> Result<(), SoundBackendError> {
        if !gain.is_finite() || gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain });
        }
        self.voice(voice)?
            .track
            .set_gain(gain)
            .map_err(|source| SoundBackendError::adapter("set sound voice gain", source))
    }

    /// Enables or clears SDL spatial mixing for one live voice.
    ///
    /// This method accepts listener-relative adapter coordinates only. It does
    /// not reinterpret world axes or substitute SDL's distance curve for the
    /// stock min/max-distance and cone policy owned by the engine.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn set_spatial_position(
        &self,
        voice: SoundVoiceHandle,
        position: Option<SoundSpatialPosition>,
    ) -> Result<(), SoundBackendError> {
        let track = &self.voice(voice)?.track;
        match position {
            Some(position) => {
                let [x, y, z] = position.coordinates();
                track
                    .set_3d_position(Point3D { x, y, z })
                    .map_err(|source| SoundBackendError::adapter("position sound voice", source))
            }
            None => track
                .set_stereo(None)
                .map_err(|source| SoundBackendError::adapter("clear sound voice position", source)),
        }
    }

    /// Stops one voice immediately and releases its decoded input reference.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn stop(&self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        let track = &self.voice(voice)?.track;
        track
            .stop(0)
            .map_err(|source| SoundBackendError::adapter("stop sound voice", source))?;
        track
            .clear_audio()
            .map_err(|source| SoundBackendError::adapter("release sound voice input", source))
    }

    /// Pulls mixed bytes from the borrowed output.
    ///
    /// # Errors
    ///
    /// Returns the number of non-silence bytes mixed; the remaining requested
    /// bytes are initialized to silence.
    ///
    /// Returns [`SoundBackendError`] when the output targets a device or SDL
    /// reports generation failure.
    pub fn generate(&self, buffer: &mut [u8]) -> Result<usize, SoundBackendError> {
        self.output.generate(buffer)
    }

    /// Resolves a handle only when both backend and generation still match.
    fn voice(&self, voice: SoundVoiceHandle) -> Result<&VoiceSlot<'_>, SoundBackendError> {
        if voice.backend_id != self.backend_id {
            return Err(SoundBackendError::UnknownVoice);
        }
        let slot = self
            .voices
            .get(usize::from(voice.slot))
            .ok_or(SoundBackendError::UnknownVoice)?;
        if slot.generation != voice.generation {
            return Err(SoundBackendError::UnknownVoice);
        }
        Ok(slot)
    }
}

impl Drop for SoundBackend<'_> {
    fn drop(&mut self) {
        for voice in &self.voices {
            let _stop_result = voice.track.stop(0);
        }
        self.voices.clear();
    }
}
