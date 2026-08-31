//! Stable sound-engine orchestration and settings failures.

use solarity_asset::AssetError;
use thiserror::Error;

use crate::audio::backend::SoundBackendError;
use crate::audio::codec::SoundDecodeError;

/// Invalid stock master or category CVar gain.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
#[error("sound gain must be finite and in 0..=1, got {value}")]
pub struct SoundGainError {
    /// Unmodified caller-provided value.
    pub(super) value: f32,
}

/// Failure while selecting, admitting, or controlling a stock sound.
#[derive(Debug, Error)]
pub enum SoundEngineError {
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
    /// A voice handle is not currently owned by this engine.
    #[error("sound voice is not owned by this engine")]
    UnknownVoice,
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
