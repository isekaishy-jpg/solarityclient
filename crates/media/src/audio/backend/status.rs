//! Stable failures at the output and voice-management adapter boundary.

use thiserror::Error;

/// Failure to open an output or control one backend voice.
#[derive(Debug, Error)]
pub enum SoundBackendError {
    /// SDL initialization, output, track, or playback control failed.
    #[error("{operation} failed: {message}")]
    Adapter {
        /// Stable operation name owned by this crate.
        operation: &'static str,
        /// Dependency diagnostic retained for operator context.
        message: String,
    },
    /// The dependency returned a format that cannot represent playable audio.
    #[error("sound output has invalid format: {sample_rate_hz} Hz and {channel_count} channels")]
    InvalidOutputFormat {
        /// Unmodified dependency sample rate.
        sample_rate_hz: i32,
        /// Unmodified dependency channel count.
        channel_count: i32,
    },
    /// A decoded-sound handle did not belong to the supplied decoder.
    #[error("decoded sound handle does not belong to this decoder")]
    UnknownSound,
    /// A voice handle is stale or belongs to another backend.
    #[error("sound voice handle is stale or belongs to another backend")]
    UnknownVoice,
    /// Every explicitly configured track is still playing or paused.
    #[error("configured sound voice capacity is exhausted")]
    VoiceCapacity,
    /// A requested gain was negative or non-finite.
    #[error("sound voice gain must be finite and nonnegative, got {gain}")]
    InvalidGain {
        /// Unmodified caller-provided gain.
        gain: f32,
    },
    /// A listener-relative spatial coordinate was non-finite.
    #[error("sound voice spatial position must be finite, got {position:?}")]
    InvalidSpatialPosition {
        /// Unmodified right, up, and back coordinates.
        position: [f32; 3],
    },
    /// A slot has been reused more times than its stable handle can encode.
    #[error("sound voice generation capacity is exhausted")]
    GenerationCapacity,
    /// Memory generation was requested from a device output.
    #[error("sound output is not a memory target")]
    NotMemoryOutput,
}

impl SoundBackendError {
    /// Adds one stable operation name to an SDL-owned error string.
    pub(super) fn adapter(operation: &'static str, source: impl ToString) -> Self {
        Self::Adapter {
            operation,
            message: source.to_string(),
        }
    }
}
