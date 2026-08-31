//! Stable failures at the encoded-audio adapter boundary.

use solarity_asset::AssetPath;
use thiserror::Error;

/// Failure to admit one encoded stock sound into SDL3_mixer.
#[derive(Debug, Error)]
pub enum SoundDecodeError {
    /// SDL initialization, stream creation, decoding, or format inspection failed.
    #[error("{operation} failed: {message}")]
    Adapter {
        /// Stable operation name owned by this crate.
        operation: &'static str,
        /// Dependency diagnostic retained for operator context.
        message: String,
    },
    /// The dependency returned a format that cannot represent playable audio.
    #[error(
        "decoded sound {path} has invalid format: {sample_rate_hz} Hz and {channel_count} channels"
    )]
    InvalidFormat {
        /// Exact encoded-source identity.
        path: AssetPath,
        /// Unmodified dependency sample rate.
        sample_rate_hz: i32,
        /// Unmodified dependency channel count.
        channel_count: i32,
    },
    /// More resources were admitted than the stable handle can address.
    #[error("decoded sound registry capacity exceeded")]
    Capacity,
}

impl SoundDecodeError {
    /// Adds one stable operation name to an SDL-owned error string.
    pub(super) fn adapter(operation: &'static str, source: impl ToString) -> Self {
        Self::Adapter {
            operation,
            message: source.to_string(),
        }
    }
}
