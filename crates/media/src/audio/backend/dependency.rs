//! SDL3_mixer adapter replacing the stock client's FMOD output boundary.

#![allow(unsafe_code)]

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::mixer::{Mixer, Point3D, StereoGains, Track};
use sdl3::properties::{Properties, Setter};
use sdl3::sys::audio::{SDL_AUDIO_S16LE, SDL_AudioSpec};

use crate::audio::codec::{DecodedSoundHandle, SoundDecoder};

use super::status::SoundBackendError;
use super::types::{
    SoundBackendPlayback, SoundOutputConfiguration, SoundOutputDevice, SoundOutputDeviceId,
    SoundOutputInfo, SoundOutputQuality, SoundOutputTarget, SoundSpatialPosition, SoundVoiceHandle,
    SoundVoicePriority, SoundVoiceState,
};

const STOCK_OUTPUT_CHANNEL_COUNT: i32 = 2;

/// One reusable SDL track and the generation currently exposed to callers.
struct VoiceSlot<'output> {
    track: Track<'output>,
    generation: u32,
    logical_gain: f32,
    /// Last successfully applied SDL gain, including virtual-voice silence.
    applied_gain: f32,
    /// Cleared tracks cannot participate in playback or require SDL queries.
    has_audio: bool,
    priority_word: i32,
    priority: u16,
    looping: bool,
    admission_sequence: u64,
    virtualized: bool,
}

/// One FFmpeg-fed SDL stream whose track is destroyed before its input.
struct CinematicTrack<'output> {
    track: Track<'output>,
    _stream: AudioStreamOwner,
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
    /// Enumerates the current physical outputs in the platform's stable order.
    ///
    /// # Errors
    /// Returns SDL initialization, enumeration, or device-name failures.
    pub fn devices() -> Result<Vec<SoundOutputDevice>, SoundBackendError> {
        let sdl = sdl3::init().map_err(|source| {
            SoundBackendError::adapter("retain SDL for sound enumeration", source)
        })?;
        let audio = sdl.audio().map_err(|source| {
            SoundBackendError::adapter("retain audio for sound enumeration", source)
        })?;
        let devices = audio
            .audio_playback_device_ids()
            .map_err(|source| SoundBackendError::adapter("enumerate sound outputs", source))?;
        devices
            .into_iter()
            .map(|device| {
                Ok(SoundOutputDevice {
                    id: SoundOutputDeviceId(device.id().0),
                    name: device.name().map_err(|source| {
                        SoundBackendError::adapter("read sound output name", source)
                    })?,
                })
            })
            .collect()
    }

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
        Self::open_configured(SoundOutputConfiguration {
            target,
            quality: SoundOutputQuality::Medium,
        })
    }

    /// Opens the exact selected device and native quality format.
    ///
    /// # Errors
    /// Returns an output creation or actual-format validation failure.
    pub fn open_configured(
        configuration: SoundOutputConfiguration,
    ) -> Result<Self, SoundBackendError> {
        let target = configuration.target;
        let requested_format = SDL_AudioSpec {
            format: SDL_AUDIO_S16LE,
            channels: STOCK_OUTPUT_CHANNEL_COUNT,
            freq: configuration.quality.sample_rate_hz() as i32,
        };
        let mixer = match target {
            SoundOutputTarget::DefaultDevice => Mixer::open_device(Some(&requested_format)),
            SoundOutputTarget::Device(id) => Mixer::open_device_id(
                sdl3::sys::audio::SDL_AudioDeviceID(id.0),
                Some(&requested_format),
            ),
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

/// Fixed-capacity owner of reusable virtual voices on one borrowed output.
///
/// Build 12340 gives FMOD a hard virtual pool independently of the smaller
/// `Sound_NumChannels` software-mix count. SDL has no matching virtual-voice
/// facility, so every logical voice retains a track and voices outside the
/// software set advance silently at zero gain.
pub struct SoundBackend<'output> {
    loop_options: Properties,
    backend_id: u64,
    output: &'output SoundOutput,
    voices: Vec<VoiceSlot<'output>>,
    cinematic: Option<CinematicTrack<'output>>,
    software_channel_count: usize,
    next_admission_sequence: u64,
}

impl<'output> SoundBackend<'output> {
    /// Native background policy changes bus gain while voices keep advancing.
    pub(in crate::audio) fn set_background_muted(
        &self,
        muted: bool,
    ) -> Result<(), SoundBackendError> {
        self.output
            .mixer
            .set_gain(if muted { 0.0 } else { 1.0 })
            .map_err(|source| SoundBackendError::adapter("apply background sound gain", source))
    }

    /// Recreates stopped tracks on a new output while old generation queries
    /// remain valid. Native 0087DED0 retires playback before the device reopens.
    pub(in crate::audio) fn restarted(
        &self,
        output: &'output SoundOutput,
        software_channel_count: NonZeroU16,
    ) -> Result<Self, SoundBackendError> {
        let capacity = u16::try_from(self.voices.len())
            .ok()
            .and_then(NonZeroU16::new)
            .ok_or(SoundBackendError::VoiceCapacity)?;
        let mut replacement = Self::new(output, software_channel_count, capacity)?;
        replacement.backend_id = self.backend_id;
        replacement.next_admission_sequence = self.next_admission_sequence;
        for (next, previous) in replacement.voices.iter_mut().zip(&self.voices) {
            next.generation = previous.generation;
        }
        Ok(replacement)
    }

    /// Allocates the complete voice pool on an already opened output.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] when SDL cannot create every explicitly
    /// requested track.
    pub fn new(
        output: &'output SoundOutput,
        software_channel_count: NonZeroU16,
        voice_capacity: NonZeroU16,
    ) -> Result<Self, SoundBackendError> {
        static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(1);

        if software_channel_count > voice_capacity {
            return Err(SoundBackendError::InvalidChannelLimits {
                software: software_channel_count.get(),
                virtual_voices: voice_capacity.get(),
            });
        }

        let loop_options = Properties::new().map_err(|source| {
            SoundBackendError::adapter("create sound loop options", format!("{source:?}"))
        })?;
        // MIX_PROP_PLAY_LOOPS_NUMBER from SDL_mixer.h. MIX_PlayTrack resets
        // loops, so MIX_SetTrackLoops before play cannot configure a new voice.
        loop_options
            .set("SDL_mixer.play.loops", -1_i64)
            .map_err(|source| {
                SoundBackendError::adapter("configure sound loops", format!("{source:?}"))
            })?;
        let mut voices = Vec::with_capacity(usize::from(voice_capacity.get()));
        for _slot in 0..voice_capacity.get() {
            let track = output
                .mixer
                .create_track()
                .map_err(|source| SoundBackendError::adapter("create sound voice", source))?;
            voices.push(VoiceSlot {
                track,
                generation: 0,
                logical_gain: 0.0,
                applied_gain: 0.0,
                has_audio: false,
                priority_word: SoundVoicePriority::DEFAULT.value(),
                priority: SoundVoicePriority::DEFAULT.effective(),
                looping: false,
                admission_sequence: 0,
                virtualized: false,
            });
        }
        Ok(Self {
            backend_id: NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed),
            loop_options,
            output,
            voices,
            cinematic: None,
            software_channel_count: usize::from(software_channel_count.get()),
            next_admission_sequence: 0,
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

    /// Returns the `Sound_NumChannels` real software-mix count.
    #[must_use]
    pub const fn software_channel_count(&self) -> usize {
        self.software_channel_count
    }

    /// Starts the dedicated non-spatial cinematic stream with initial PCM.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] when SDL cannot create, attach, feed, or
    /// start the stream, or when gain is negative or non-finite.
    pub fn start_cinematic_audio(
        &mut self,
        samples: &[i16],
        gain: f32,
    ) -> Result<(), SoundBackendError> {
        if !gain.is_finite() || gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain });
        }
        self.stop_cinematic_audio()?;
        let sdl = sdl3::init()
            .map_err(|source| SoundBackendError::adapter("retain SDL for movie audio", source))?;
        let audio = sdl.audio().map_err(|source| {
            SoundBackendError::adapter("retain SDL audio for movie stream", source)
        })?;
        let source_spec = AudioSpec::new(Some(44_100), Some(2), Some(AudioFormat::s16_sys()));
        let output_info = self.output.info();
        let output_rate = i32::try_from(output_info.sample_rate_hz()).map_err(|source| {
            SoundBackendError::adapter("convert movie output sample rate", source)
        })?;
        let output_spec = AudioSpec::new(
            Some(output_rate),
            Some(i32::from(output_info.channel_count())),
            Some(AudioFormat::s16_sys()),
        );
        let mut stream = audio
            .new_stream(Some(&source_spec), Some(&output_spec))
            .map_err(|source| SoundBackendError::adapter("create movie audio stream", source))?;
        stream
            .put_data_i16(samples)
            .map_err(|source| SoundBackendError::adapter("queue initial movie audio", source))?;
        let track = self
            .output
            .mixer
            .create_track()
            .map_err(|source| SoundBackendError::adapter("create movie audio track", source))?;
        // SAFETY: `stream` is moved into `CinematicTrack` after attachment and
        // its field follows `track`, so Rust destroys the track before stream.
        unsafe { track.set_audio_stream(stream.stream()) }
            .map_err(|source| SoundBackendError::adapter("attach movie audio stream", source))?;
        track
            .set_gain(gain)
            .and_then(|()| track.play())
            .map_err(|source| SoundBackendError::adapter("start movie audio track", source))?;
        self.cinematic = Some(CinematicTrack {
            track,
            _stream: stream,
        });
        Ok(())
    }

    /// Appends decoded PCM to the active cinematic stream.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] when no movie track exists or SDL rejects
    /// the queued samples.
    pub fn queue_cinematic_audio(&mut self, samples: &[i16]) -> Result<(), SoundBackendError> {
        let cinematic = self
            .cinematic
            .as_ref()
            .ok_or(SoundBackendError::MissingCinematicTrack)?;
        cinematic
            ._stream
            .put_data_i16(samples)
            .map_err(|source| SoundBackendError::adapter("queue movie audio", source))
    }

    /// Returns the output-consumed position of the active cinematic track.
    ///
    /// SDL reports this in sample frames. Stock's movie update at
    /// `CSimpleMovieFrame.cpp` 0x0095EBF0 likewise selects audio as the master
    /// clock while its movie channel is active.
    #[must_use]
    pub fn cinematic_playback_time(&self) -> Option<Duration> {
        let cinematic = self.cinematic.as_ref()?;
        let frames = cinematic.track.playback_position();
        if frames < 0 {
            return None;
        }
        let milliseconds = cinematic.track.frames_to_ms(frames);
        u64::try_from(milliseconds).ok().map(Duration::from_millis)
    }

    /// Stops and releases the dedicated cinematic stream when one is active.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] if SDL cannot stop the live track.
    pub fn stop_cinematic_audio(&mut self) -> Result<(), SoundBackendError> {
        let Some(cinematic) = self.cinematic.take() else {
            return Ok(());
        };
        cinematic
            .track
            .stop(0)
            .map_err(|source| SoundBackendError::adapter("stop movie audio track", source))
    }

    /// Starts one decoded sound on a stopped or weakest virtual slot.
    ///
    /// A gain above one is retained because SDL and stock source-volume policy
    /// both permit amplification. Only negative and non-finite gains are
    /// rejected at this low-level boundary.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a foreign sound, invalid gain,
    /// identity-capacity exhaustion, or SDL playback failure.
    pub fn play(
        &mut self,
        decoder: &SoundDecoder,
        sound: DecodedSoundHandle,
        gain: f32,
        looping: bool,
        priority: SoundVoicePriority,
    ) -> Result<SoundBackendPlayback, SoundBackendError> {
        self.play_prepared(
            decoder,
            sound,
            super::SoundVoiceStart {
                gain,
                looping,
                priority,
                frequency_ratio: 1.0,
                position: None,
            },
        )
    }

    pub(in crate::audio) fn play_prepared(
        &mut self,
        decoder: &SoundDecoder,
        sound: DecodedSoundHandle,
        options: super::SoundVoiceStart,
    ) -> Result<SoundBackendPlayback, SoundBackendError> {
        let super::SoundVoiceStart {
            gain,
            looping,
            priority,
            frequency_ratio,
            position,
        } = options;
        if !gain.is_finite() || gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain });
        }
        if !frequency_ratio.is_finite() || frequency_ratio <= 0.0 {
            return Err(SoundBackendError::InvalidFrequencyRatio {
                ratio: frequency_ratio,
            });
        }
        let audio = decoder
            .audio(sound)
            .ok_or(SoundBackendError::UnknownSound)?;
        let stopped_slot = self
            .voices
            .iter()
            .position(|slot| !slot.track.is_playing() && !slot.track.is_paused());
        let slot_index = stopped_slot
            .or_else(|| self.weakest_voice_index())
            .ok_or(SoundBackendError::VoiceCapacity)?;
        // Output playback can finish after the engine's last collection pass.
        // Reusing that stopped slot still invalidates its previous generation;
        // report it so the engine releases the old voice and decoded reference.
        let replaced =
            (self.voices[slot_index].generation != 0).then(|| self.handle_for_slot(slot_index));
        let admission_sequence = self
            .next_admission_sequence
            .checked_add(1)
            .ok_or(SoundBackendError::AdmissionSequenceCapacity)?;
        let slot = &mut self.voices[slot_index];
        let generation = slot
            .generation
            .checked_add(1)
            .ok_or(SoundBackendError::GenerationCapacity)?;

        if replaced.is_some() {
            slot.track
                .stop(0)
                .and_then(|()| slot.track.clear_audio())
                .map_err(|source| SoundBackendError::adapter("replace sound voice", source))?;
        }

        if let Err(source) = slot.track.set_audio(audio) {
            return Err(SoundBackendError::adapter(
                "assign sound voice input",
                source,
            ));
        }
        let result = slot
            .track
            .set_gain(0.0)
            .and_then(|()| slot.track.set_stereo(None))
            .and_then(|()| slot.track.set_frequency_ratio(frequency_ratio))
            .and_then(|()| match position {
                Some(position) => {
                    let [x, y, z] = position.coordinates();
                    slot.track.set_3d_position(Point3D { x, y, z })
                }
                None => Ok(()),
            })
            .and_then(|()| {
                if looping {
                    slot.track.play_with_options(&self.loop_options)
                } else {
                    slot.track.play()
                }
            });
        if let Err(source) = result {
            let _cleanup_result = slot.track.clear_audio();
            return Err(SoundBackendError::adapter("start sound voice", source));
        }
        slot.generation = generation;
        slot.logical_gain = gain;
        slot.applied_gain = 0.0;
        slot.has_audio = true;
        slot.priority_word = priority.value();
        slot.priority = priority.effective();
        slot.looping = looping;
        slot.admission_sequence = admission_sequence;
        slot.virtualized = true;
        self.next_admission_sequence = admission_sequence;
        let voice = SoundVoiceHandle {
            backend_id: self.backend_id,
            slot: slot_index as u16,
            generation,
        };
        self.rebalance()?;
        Ok(SoundBackendPlayback { voice, replaced })
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

    /// Reports whether a playing logical voice is outside the software mix.
    ///
    /// This mirrors FMOD Channel::isVirtual. Paused and stopped voices return
    /// false because neither participates in real-voice ordering.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError::UnknownVoice`] for a stale or foreign
    /// handle.
    pub fn is_virtual(&self, voice: SoundVoiceHandle) -> Result<bool, SoundBackendError> {
        let slot = self.voice(voice)?;
        Ok(slot.track.is_playing() && !slot.track.is_paused() && slot.virtualized)
    }

    /// Returns the priority currently used for real/virtual ordering.
    ///
    /// A default non-looping voice can report 127 after stock's real-voice
    /// promotion pass; all other requests retain their resolved bucket.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError::UnknownVoice`] for a stale or foreign
    /// handle.
    pub fn effective_priority(&self, voice: SoundVoiceHandle) -> Result<u16, SoundBackendError> {
        Ok(self.voice(voice)?.priority)
    }

    /// Pauses one live voice without changing its playback position.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn pause(&mut self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        self.voice(voice)?
            .track
            .pause()
            .map_err(|source| SoundBackendError::adapter("pause sound voice", source))?;
        self.rebalance()
    }

    /// Resumes one paused live voice.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for a stale handle or SDL failure.
    pub fn resume(&mut self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        self.voice(voice)?
            .track
            .resume()
            .map_err(|source| SoundBackendError::adapter("resume sound voice", source))?;
        self.rebalance()
    }

    /// Changes one live voice's nonnegative gain without clamping it.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for an invalid gain, stale handle, or SDL
    /// failure.
    pub fn set_gain(
        &mut self,
        voice: SoundVoiceHandle,
        gain: f32,
    ) -> Result<(), SoundBackendError> {
        if !gain.is_finite() || gain < 0.0 {
            return Err(SoundBackendError::InvalidGain { gain });
        }
        self.voice_mut(voice)?.logical_gain = gain;
        self.rebalance()
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
        self.set_spatial_mix(voice, position, 1.0)
    }

    /// Applies stock's 2D-to-3D pan blend at the SDL adapter boundary.
    ///
    /// Stereo output uses the same constant-power directional gains as SDL's
    /// 3D panner and linearly blends its matrix with centered 2D gains. Other
    /// output layouts retain SDL's native spatializer until a layout-specific
    /// matrix boundary is introduced.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`] for an invalid level, stale handle, or SDL
    /// failure.
    pub fn set_spatial_mix(
        &self,
        voice: SoundVoiceHandle,
        position: Option<SoundSpatialPosition>,
        pan_level: f32,
    ) -> Result<(), SoundBackendError> {
        if !pan_level.is_finite() || !(0.0..=1.0).contains(&pan_level) {
            return Err(SoundBackendError::InvalidSpatialPanLevel { level: pan_level });
        }
        let track = &self.voice(voice)?.track;
        match position {
            Some(position) => {
                let [x, y, z] = position.coordinates();
                if self.output.info.channel_count() == 2 && pan_level < 1.0 {
                    let [spatial_left, spatial_right] = stereo_direction_gains(x, z);
                    const CENTER_GAIN: f32 = core::f32::consts::FRAC_1_SQRT_2;
                    let center_level = 1.0 - pan_level;
                    track
                        .set_stereo(Some(StereoGains {
                            left: CENTER_GAIN * center_level + spatial_left * pan_level,
                            right: CENTER_GAIN * center_level + spatial_right * pan_level,
                        }))
                        .map_err(|source| {
                            SoundBackendError::adapter("blend sound voice position", source)
                        })
                } else {
                    track
                        .set_3d_position(Point3D { x, y, z })
                        .map_err(|source| {
                            SoundBackendError::adapter("position sound voice", source)
                        })
                }
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
    pub fn stop(&mut self, voice: SoundVoiceHandle) -> Result<(), SoundBackendError> {
        let slot = self.voice_mut(voice)?;
        slot.track
            .stop(0)
            .map_err(|source| SoundBackendError::adapter("stop sound voice", source))?;
        slot.track
            .clear_audio()
            .map_err(|source| SoundBackendError::adapter("release sound voice input", source))?;
        slot.logical_gain = 0.0;
        slot.has_audio = false;
        slot.virtualized = false;
        self.rebalance()
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

    /// Resolves one mutable slot only for its current generation.
    fn voice_mut(
        &mut self,
        voice: SoundVoiceHandle,
    ) -> Result<&mut VoiceSlot<'output>, SoundBackendError> {
        if voice.backend_id != self.backend_id {
            return Err(SoundBackendError::UnknownVoice);
        }
        let slot = self
            .voices
            .get_mut(usize::from(voice.slot))
            .ok_or(SoundBackendError::UnknownVoice)?;
        if slot.generation != voice.generation {
            return Err(SoundBackendError::UnknownVoice);
        }
        Ok(slot)
    }

    /// Returns the least important active voice at hard-pool exhaustion.
    fn weakest_voice_index(&self) -> Option<usize> {
        let mut indices = (0..self.voices.len()).collect::<Vec<_>>();
        self.sort_real_voice_order(&mut indices);
        indices.last().copied()
    }

    /// Applies FMOD's priority buckets and equal-priority audibility ordering.
    fn rebalance(&mut self) -> Result<(), SoundBackendError> {
        let mut indices = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_index, slot)| {
                slot.has_audio && slot.track.is_playing() && !slot.track.is_paused()
            })
            .map(|(index, _slot)| index)
            .collect::<Vec<_>>();
        self.sort_real_voice_order(&mut indices);
        let mut real = vec![false; self.voices.len()];
        for index in indices.iter().copied().take(self.software_channel_count) {
            real[index] = true;
        }
        // SoundEngine.cpp promotes only default-priority one-shots that FMOD
        // already reports as playing and real. Virtual voices remain at 128
        // until they first enter the real set.
        for (index, slot) in self.voices.iter_mut().enumerate() {
            if real[index] && !slot.looping && (slot.priority_word < 0 || slot.priority_word == 128)
            {
                slot.priority = 127;
            }
        }
        // Promotion only improves voices already inside the real set; it cannot
        // change membership. Reuse the same playback snapshot instead of sorting
        // again and repeatedly acquiring SDL's mixer lock for every pool slot.
        for slot in &mut self.voices {
            slot.virtualized = false;
        }
        for index in indices {
            let slot = &mut self.voices[index];
            slot.virtualized = !real[index];
            let gain = if real[index] { slot.logical_gain } else { 0.0 };
            if slot.applied_gain != gain {
                slot.track.set_gain(gain).map_err(|source| {
                    SoundBackendError::adapter("order virtual sound voices", source)
                })?;
                slot.applied_gain = gain;
            }
        }
        Ok(())
    }

    /// Sorts best first: lower priority, then greater current audibility.
    fn sort_real_voice_order(&self, indices: &mut [usize]) {
        indices.sort_by(|left, right| {
            let left_slot = &self.voices[*left];
            let right_slot = &self.voices[*right];
            left_slot
                .priority
                .cmp(&right_slot.priority)
                .then_with(|| right_slot.logical_gain.total_cmp(&left_slot.logical_gain))
                .then_with(|| {
                    left_slot
                        .admission_sequence
                        .cmp(&right_slot.admission_sequence)
                })
        });
    }

    /// Reconstructs the currently exposed generation for one slot.
    fn handle_for_slot(&self, slot: usize) -> SoundVoiceHandle {
        SoundVoiceHandle {
            backend_id: self.backend_id,
            slot: slot as u16,
            generation: self.voices[slot].generation,
        }
    }
}

/// Reproduces SDL's stereo constant-power quadrant panner at unit distance.
fn stereo_direction_gains(right: f32, back: f32) -> [f32; 2] {
    const QUARTER_PI: f32 = core::f32::consts::FRAC_PI_4;
    const THREE_QUARTER_PI: f32 = 3.0 * QUARTER_PI;
    const CENTER_GAIN: f32 = core::f32::consts::FRAC_1_SQRT_2;

    let radians = right.atan2(-back);
    if (-QUARTER_PI..=QUARTER_PI).contains(&radians) {
        let (sine, cosine) = radians.sin_cos();
        [CENTER_GAIN * (cosine - sine), CENTER_GAIN * (cosine + sine)]
    } else if (QUARTER_PI..=THREE_QUARTER_PI).contains(&radians) {
        [0.0, 1.0]
    } else if (-THREE_QUARTER_PI..=-QUARTER_PI).contains(&radians) {
        [1.0, 0.0]
    } else {
        let rear_angle = if radians < 0.0 {
            -(radians + core::f32::consts::PI)
        } else {
            -(radians - core::f32::consts::PI)
        };
        let (sine, cosine) = rear_angle.sin_cos();
        [CENTER_GAIN * (cosine - sine), CENTER_GAIN * (cosine + sine)]
    }
}

impl Drop for SoundBackend<'_> {
    fn drop(&mut self) {
        if let Some(cinematic) = self.cinematic.take() {
            let _stop_result = cinematic.track.stop(0);
        }
        for voice in &self.voices {
            let _stop_result = voice.track.stop(0);
        }
        self.voices.clear();
    }
}
