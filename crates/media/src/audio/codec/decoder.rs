//! SDL3_mixer resource admission from archive-selected memory payloads.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use sdl3::iostream::IOStream;
use sdl3::mixer::{Audio, Mixer};
use sdl3::sys::audio::{SDL_AUDIO_S16LE, SDL_AudioSpec};
use solarity_asset::AssetPath;

use crate::audio::cache::EncodedSound;

use super::status::SoundDecodeError;
use super::types::{DecodedSoundHandle, DecodedSoundInfo, SoundDecodeMode};

/// Resource identity includes decode mode because it changes retained storage.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DecodedSoundKey {
    path: AssetPath,
    mode: SoundDecodeMode,
}

/// One SDL-owned audio object and its dependency-neutral description.
struct DecodedSoundResource {
    audio: Audio,
    info: DecodedSoundInfo,
}

/// Single-threaded decoded-audio registry independent of an output device.
///
/// SDL documents `Audio` objects as shareable across mixer devices. Loading
/// against a memory mixer therefore admits and deduplicates resources before a
/// playback engine creates its output tracks.
pub struct SoundDecoder {
    decoder_id: u64,
    mixer: Mixer,
    handles: HashMap<DecodedSoundKey, DecodedSoundHandle>,
    resources: Vec<DecodedSoundResource>,
}

impl SoundDecoder {
    /// Initializes SDL3_mixer and a device-independent memory mixer.
    ///
    /// # Errors
    ///
    /// Returns [`SoundDecodeError`] when SDL3_mixer cannot initialize or create
    /// the memory mixer.
    pub fn new() -> Result<Self, SoundDecodeError> {
        static NEXT_DECODER_ID: AtomicU64 = AtomicU64::new(1);
        const STOCK_OUTPUT_SAMPLE_RATE_HZ: i32 = 44_100;
        const STOCK_OUTPUT_CHANNEL_COUNT: i32 = 2;

        // The pinned wrapper currently forwards a null format that SDL rejects
        // for memory mixers. Build 12340's Sound_OutputSampleRate defaults to
        // 44.1 kHz, and its minimum supported output is 16-bit stereo.
        let mix_format = SDL_AudioSpec {
            format: SDL_AUDIO_S16LE,
            channels: STOCK_OUTPUT_CHANNEL_COUNT,
            freq: STOCK_OUTPUT_SAMPLE_RATE_HZ,
        };
        let mixer = Mixer::create_memory(Some(&mix_format))
            .map_err(|source| SoundDecodeError::adapter("create memory audio mixer", source))?;
        Ok(Self {
            decoder_id: NEXT_DECODER_ID.fetch_add(1, Ordering::Relaxed),
            mixer,
            handles: HashMap::new(),
            resources: Vec::new(),
        })
    }

    /// Returns the number of distinct path/mode resources retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Reports whether no SDL audio resources are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Admits one encoded payload in the caller-selected residency mode.
    ///
    /// SDL3_mixer copies the original encoded file during this call. The input
    /// stream can retire immediately, and subsequent requests for the same
    /// normalized path and mode reuse the existing audio object.
    ///
    /// # Errors
    ///
    /// Returns [`SoundDecodeError`] for empty/unsupported encoded data, decoder
    /// failures, invalid reported formats, or handle-capacity exhaustion.
    pub fn load(
        &mut self,
        encoded: &EncodedSound,
        mode: SoundDecodeMode,
    ) -> Result<DecodedSoundHandle, SoundDecodeError> {
        let key = DecodedSoundKey {
            path: encoded.path().clone(),
            mode,
        };
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }
        let slot =
            u32::try_from(self.resources.len()).map_err(|_source| SoundDecodeError::Capacity)?;
        let stream = IOStream::from_bytes(encoded.bytes())
            .map_err(|source| SoundDecodeError::adapter("open encoded sound memory", source))?;
        let audio = self
            .mixer
            .load_audio_io(&stream, mode == SoundDecodeMode::Predecoded)
            .map_err(|source| SoundDecodeError::adapter("decode encoded sound", source))?;
        let format = audio
            .format()
            .map_err(|source| SoundDecodeError::adapter("inspect decoded sound format", source))?;
        let invalid_format = || SoundDecodeError::InvalidFormat {
            path: encoded.path().clone(),
            sample_rate_hz: format.freq,
            channel_count: format.channels,
        };
        let sample_rate_hz = u32::try_from(format.freq).map_err(|_source| invalid_format())?;
        let channel_count = u8::try_from(format.channels).map_err(|_source| invalid_format())?;
        if sample_rate_hz == 0 || channel_count == 0 {
            return Err(invalid_format());
        }
        let duration = audio.duration();
        let duration_frames = (duration >= 0).then_some(duration as u64);
        let info = DecodedSoundInfo::new(
            encoded.path().clone(),
            mode,
            sample_rate_hz,
            channel_count,
            duration_frames,
        );
        let handle = DecodedSoundHandle {
            decoder_id: self.decoder_id,
            slot,
        };
        self.resources.push(DecodedSoundResource { audio, info });
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Resolves diagnostics only for handles created by this decoder.
    #[must_use]
    pub fn info(&self, handle: DecodedSoundHandle) -> Option<&DecodedSoundInfo> {
        if handle.decoder_id != self.decoder_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| &resource.info)
    }

    /// Resolves the SDL resource only inside the media crate's backend adapter.
    pub(in crate::audio) fn audio(&self, handle: DecodedSoundHandle) -> Option<&Audio> {
        if handle.decoder_id != self.decoder_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| &resource.audio)
    }
}
