//! Owned CPU decoding jobs whose mixer remains alive until every result retires.

#![allow(unsafe_code)]

use std::sync::Arc;

use sdl3::iostream::IOStream;
use sdl3::mixer::{Audio, Mixer};
use sdl3::sys::audio::{SDL_AUDIO_S16LE, SDL_AudioSpec};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};

use crate::audio::cache::EncodedSound;

use super::{DecodedSoundInfo, SoundDecodeError, SoundDecodeMode};

/// One exclusively owned audio resource, before its registry admission.
pub(super) struct PreparedSound {
    pub(super) audio: Audio,
    pub(super) info: DecodedSoundInfo,
    pub(super) encoded_size_bytes: usize,
}

// SAFETY: MIX_LoadAudio_IO copies the input and produces mixer-independent
// audio. MIX_GetAudioFormat/GetAudioDuration/DestroyAudio allow any thread.
// This private value has sole Rust ownership, exposes no cross-thread borrows,
// and only travels through tasks retained by SoundLoader. That owner joins and
// destroys all task results before its mixer can release library initialization.
// https://wiki.libsdl.org/SDL3_mixer/MIX_LoadAudio_IO
// https://wiki.libsdl.org/SDL3_mixer/MIX_DestroyAudio
unsafe impl Send for PreparedSound {}

/// Shared memory-mixer ownership restricted to SDL's documented loading API.
struct LoadingMixer(Arc<Mixer>);

// SAFETY: Workers only borrow the immutable Mixer to call MIX_LoadAudio_IO,
// documented as safe from any thread. SoundLoader holds the original Arc and
// joins every job before dropping it, so worker Arc drops are never the last
// reference. MixerContext initialization/shutdown and device/track operations
// stay on the owning thread. No general Send/Sync mixer access is exposed.
unsafe impl Send for LoadingMixer {}

impl LoadingMixer {
    /// Limits the shared mixer to SDL's thread-safe audio loading operation.
    fn prepare(
        self,
        encoded: &EncodedSound,
        mode: SoundDecodeMode,
    ) -> Result<PreparedSound, SoundDecodeError> {
        prepare_sound(&self.0, encoded, mode)
    }
}

/// Non-reused identity local to one decoder's retained job owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::audio) struct SoundDecodeTicket(u64);

/// Cancellation drops the eventual resource without admitting it to the registry.
struct PendingDecode {
    ticket: SoundDecodeTicket,
    task: CpuTask<Result<PreparedSound, SoundDecodeError>>,
    cancelled: bool,
}

/// Keeps SDL initialization and every worker borrowing the mixer in one lifetime.
pub(super) struct SoundLoader {
    pending: Vec<PendingDecode>,
    next_ticket: u64,
    mixer: Arc<Mixer>,
}

impl SoundLoader {
    /// Creates the stock-format memory mixer on the caller's SDL-owning thread.
    pub(super) fn new() -> Result<Self, SoundDecodeError> {
        // Build 12340 uses 44.1 kHz and a signed-16 stereo minimum. The pinned
        // wrapper requires an explicit format when creating a memory mixer.
        let format = SDL_AudioSpec {
            format: SDL_AUDIO_S16LE,
            channels: 2,
            freq: 44_100,
        };
        let mixer = Mixer::create_memory(Some(&format))
            .map_err(|source| SoundDecodeError::adapter("create memory audio mixer", source))?;
        // Only LoadingMixer can transfer a shared reference to a worker; the
        // original owner remains !Send and outlives every such reference.
        #[allow(clippy::arc_with_non_send_sync)]
        let mixer = Arc::new(mixer);
        Ok(Self {
            pending: Vec::new(),
            next_ticket: 0,
            mixer,
        })
    }

    /// Uses the same decoder admission for callers whose API explicitly waits.
    pub(super) fn prepare(
        &self,
        encoded: &EncodedSound,
        mode: SoundDecodeMode,
    ) -> Result<PreparedSound, SoundDecodeError> {
        prepare_sound(&self.mixer, encoded, mode)
    }

    /// Retains every accepted task; a full CPU pool leaves submission to the caller.
    pub(super) fn submit(
        &mut self,
        cpu: &CpuExecutor,
        encoded: Arc<EncodedSound>,
        mode: SoundDecodeMode,
    ) -> Result<Option<SoundDecodeTicket>, SoundDecodeError> {
        let next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or(SoundDecodeError::Capacity)?;
        let mixer = LoadingMixer(Arc::clone(&self.mixer));
        let task = match cpu.try_submit(move || {
            let timing =
                std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| std::time::Instant::now());
            let result = mixer.prepare(&encoded, mode);
            if let Some(start) = timing {
                tracing::info!(path = %encoded.path(), ?mode,
                    elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0,
                    "prepared sound resource on worker");
            }
            result
        }) {
            Ok(task) => task,
            Err(CpuError::AtCapacity { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let ticket = SoundDecodeTicket(self.next_ticket);
        self.next_ticket = next_ticket;
        self.pending.push(PendingDecode {
            ticket,
            task,
            cancelled: false,
        });
        Ok(Some(ticket))
    }

    /// Takes a completed live job without waiting for an unfinished decode.
    pub(super) fn poll(
        &mut self,
        ticket: SoundDecodeTicket,
    ) -> Result<Option<PreparedSound>, SoundDecodeError> {
        let index = self.live_index(ticket)?;
        if !self.pending[index].task.is_finished() {
            return Ok(None);
        }
        self.take(index).map(Some)
    }

    /// Completes an already submitted job for the explicitly synchronous API.
    pub(super) fn finish(
        &mut self,
        ticket: SoundDecodeTicket,
    ) -> Result<PreparedSound, SoundDecodeError> {
        let index = self.live_index(ticket)?;
        self.take(index)
    }

    /// Marks a job for disposal while preserving ownership of its active worker.
    pub(super) fn cancel(&mut self, ticket: SoundDecodeTicket) {
        if let Some(job) = self.pending.iter_mut().find(|job| job.ticket == ticket) {
            job.cancelled = true;
        }
    }

    /// Releases completed cancelled resources without pausing an interactive frame.
    pub(super) fn collect_cancelled(&mut self) {
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].cancelled && self.pending[index].task.is_finished() {
                self.discard(index);
            } else {
                index += 1;
            }
        }
    }

    /// Observes cancelled workers at the explicit shutdown boundary.
    pub(super) fn finish_cancelled(&mut self) {
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].cancelled {
                self.discard(index);
            } else {
                index += 1;
            }
        }
    }

    /// Rejects stale ticket use before touching another task's resource.
    fn live_index(&self, ticket: SoundDecodeTicket) -> Result<usize, SoundDecodeError> {
        self.pending
            .iter()
            .position(|job| job.ticket == ticket && !job.cancelled)
            .ok_or(SoundDecodeError::UnknownLoad)
    }

    /// Joins while the mixer is still alive, even if the worker failed or panicked.
    fn take(&mut self, index: usize) -> Result<PreparedSound, SoundDecodeError> {
        self.pending.swap_remove(index).task.join()?
    }

    /// Cancellation owns failure reporting because no playback caller remains.
    fn discard(&mut self, index: usize) {
        if let Err(error) = self.take(index) {
            tracing::debug!(%error, "cancelled sound decode failed");
        }
    }
}

impl Drop for SoundLoader {
    fn drop(&mut self) {
        while !self.pending.is_empty() {
            // Do not invoke a tracing subscriber while draining the final
            // owner: an unwinding callback must not skip the remaining joins.
            let _discarded = self.take(self.pending.len() - 1);
        }
    }
}

/// Loads and validates an exact payload without accessing tracks or registry state.
fn prepare_sound(
    mixer: &Mixer,
    encoded: &EncodedSound,
    mode: SoundDecodeMode,
) -> Result<PreparedSound, SoundDecodeError> {
    let stream = IOStream::from_bytes(encoded.bytes())
        .map_err(|source| SoundDecodeError::adapter("open encoded sound memory", source))?;
    let audio = mixer
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
    let sample_rate_hz = u32::try_from(format.freq).map_err(|_| invalid_format())?;
    let channel_count = u8::try_from(format.channels).map_err(|_| invalid_format())?;
    if sample_rate_hz == 0 || channel_count == 0 {
        return Err(invalid_format());
    }
    let duration = audio.duration();
    Ok(PreparedSound {
        audio,
        info: DecodedSoundInfo::new(
            encoded.path().clone(),
            mode,
            sample_rate_hz,
            channel_count,
            (duration >= 0).then_some(duration as u64),
        ),
        encoded_size_bytes: encoded.bytes().len(),
    })
}
