//! Stable sound-engine orchestration and settings failures.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

use crate::audio::backend::SoundBackendError;
use crate::audio::codec::SoundDecodeError;

use super::AdvancedSoundSpatialError;

/// Invalid stock master or category CVar gain.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
#[error("sound gain must be finite and in 0..=1, got {value}")]
pub struct SoundGainError {
    /// Unmodified caller-provided value.
    pub(super) value: f32,
}

/// Invalid stock sound channel index.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("unsupported sound channel {value}")]
pub struct SoundChannelError {
    /// Unmodified channel word.
    pub(super) value: u32,
}

impl SoundChannelError {
    /// Returns the unrecognized channel word.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }
}

/// Failure while selecting, admitting, or controlling a stock sound.
#[derive(Debug, Error)]
pub enum SoundEngineError {
    /// The process exhausted its non-reusable asynchronous request identities.
    #[error("sound load identity capacity is exhausted")]
    LoadCapacity,
    /// A completed read does not belong to the selected request.
    #[error("sound load expected {expected}, received {actual}")]
    LoadPathMismatch {
        /// Exact path selected before the read began.
        expected: AssetPath,
        /// Actual path carried by the encoded payload.
        actual: AssetPath,
    },
    /// The requested `SoundEntries.dbc` identifier is absent.
    #[error("SoundEntries.dbc does not contain sound {entry_id}")]
    MissingEntry {
        /// Exact requested identifier.
        entry_id: u32,
    },
    /// Every authored file variation has zero selection weight.
    #[error("sound {entry_id} has no positive-weight file variation")]
    NoPlayableVariation {
        /// Exact requested identifier.
        entry_id: u32,
    },
    /// Authored source volume cannot form a backend gain.
    #[error("sound {entry_id} has invalid authored volume {volume}")]
    InvalidEntryVolume {
        /// Exact requested identifier.
        entry_id: u32,
        /// Unmodified `SoundEntries.dbc` value.
        volume: f32,
    },
    /// A stock channel reached its exact simultaneous-instance cap.
    #[error("sound channel {channel} reached its {maximum}-voice limit")]
    ChannelCapacity {
        /// Exact numeric channel index.
        channel: u8,
        /// Fixed simultaneous-instance cap.
        maximum: usize,
    },
    /// An entry carrying the exclusive bit is already active.
    #[error("exclusive sound {entry_id} is already active")]
    ExclusiveEntryActive {
        /// Exact `SoundEntries.dbc` identifier.
        entry_id: u32,
    },
    /// A voice handle is not currently owned by this engine.
    #[error("sound voice is not owned by this engine")]
    UnknownVoice,
    /// A positioned ordinary voice has invalid authored or runtime spatial data.
    #[error(transparent)]
    Spatial(#[from] AdvancedSoundSpatialError),
    /// Exact archive lookup or read failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Encoded resource admission failed.
    #[error(transparent)]
    Decode(#[from] SoundDecodeError),
    /// Output voice admission or control failed.
    #[error(transparent)]
    Backend(#[from] SoundBackendError),
}
