//! Dependency-neutral output and voice identity for the sound backend.

/// Explicit destination selected when opening the backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundOutputTarget {
    /// Mix to the operating system's default playback device.
    DefaultDevice,
    /// Mix to an application-provided memory buffer.
    ///
    /// This target supports deterministic validation and offline tools. It is
    /// never selected automatically when device creation fails.
    Memory,
}

/// Actual format accepted by the SDL mixer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundOutputInfo {
    target: SoundOutputTarget,
    sample_rate_hz: u32,
    channel_count: u8,
}

impl SoundOutputInfo {
    /// Captures the validated adapter format.
    pub(super) const fn new(
        target: SoundOutputTarget,
        sample_rate_hz: u32,
        channel_count: u8,
    ) -> Self {
        Self {
            target,
            sample_rate_hz,
            channel_count,
        }
    }

    /// Returns the explicitly opened output destination.
    #[must_use]
    pub const fn target(self) -> SoundOutputTarget {
        self.target
    }

    /// Returns the mixer's actual output frames per second.
    #[must_use]
    pub const fn sample_rate_hz(self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the mixer's actual interleaved output channel count.
    #[must_use]
    pub const fn channel_count(self) -> u8 {
        self.channel_count
    }
}

/// Stable backend-local reference to one playback voice generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SoundVoiceHandle {
    pub(super) backend_id: u64,
    pub(super) slot: u16,
    pub(super) generation: u32,
}

/// Observable playback state without exposing SDL track types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundVoiceState {
    /// The voice is contributing frames to the mixer.
    Playing,
    /// The voice retains its position but contributes no frames.
    Paused,
    /// The voice ended naturally or was stopped explicitly.
    Stopped,
}
