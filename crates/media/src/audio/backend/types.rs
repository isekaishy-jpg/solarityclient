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

/// FMOD virtual-voice priority carried by build 12340's play options.
///
/// The client initializes this word to `-1`. `SoundEngine.cpp` leaves FMOD's
/// default priority of 128 in place for negative values and values above 256;
/// values in `0..=256` are used directly. Lower effective values are more
/// important.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundVoicePriority(i32);

impl SoundVoicePriority {
    /// Stock's untouched play-option word.
    pub const DEFAULT: Self = Self(-1);

    /// Captures the unmodified signed play-option word.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Returns the unmodified play-option word.
    #[must_use]
    pub const fn value(self) -> i32 {
        self.0
    }

    /// Resolves the priority FMOD uses for virtual-voice ordering.
    #[must_use]
    pub const fn effective(self) -> u16 {
        if self.0 >= 0 && self.0 <= 256 {
            self.0 as u16
        } else {
            128
        }
    }
}

/// Result of admitting one backend voice, including any retired slot generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundBackendPlayback {
    pub(super) voice: SoundVoiceHandle,
    pub(super) replaced: Option<SoundVoiceHandle>,
}

impl SoundBackendPlayback {
    /// Returns the newly admitted voice generation.
    #[must_use]
    pub const fn voice(self) -> SoundVoiceHandle {
        self.voice
    }

    /// Returns the previous generation of the reused slot, playing or stopped.
    ///
    /// A naturally stopped voice may still be retained by the engine while
    /// another sound is loading. Admission invalidates it just as it does a
    /// playing voice replaced at the hard virtual-voice limit.
    #[must_use]
    pub const fn replaced(self) -> Option<SoundVoiceHandle> {
        self.replaced
    }
}

/// Listener-relative position accepted by the SDL spatialization boundary.
///
/// The dependency uses a right-handed listener frame: positive X is right,
/// positive Y is up, and negative Z is forward. World-to-listener conversion,
/// stock min/max distance, cone attenuation, and advanced pan-level policy all
/// remain responsibilities of the media engine rather than this adapter type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundSpatialPosition([f32; 3]);

impl SoundSpatialPosition {
    /// Validates one finite dependency-space position.
    ///
    /// # Errors
    ///
    /// Returns [`SoundBackendError`](super::SoundBackendError) when any axis is
    /// non-finite. The adapter does not replace it with the listener origin.
    pub fn new(position: [f32; 3]) -> Result<Self, super::SoundBackendError> {
        if position.iter().all(|axis| axis.is_finite()) {
            Ok(Self(position))
        } else {
            Err(super::SoundBackendError::InvalidSpatialPosition { position })
        }
    }

    /// Returns right, up, and back coordinates in listener space.
    #[must_use]
    pub const fn coordinates(self) -> [f32; 3] {
        self.0
    }
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
