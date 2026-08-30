//! Stable failures from M2 bone-pose composition.

use thiserror::Error;

/// A decoded animation set cannot produce the requested model pose.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2BonePoseError {
    /// The caller selected a sequence index outside the model catalog.
    #[error("M2 animation sequence {requested} is unavailable; model has {available} sequences")]
    SequenceIndex {
        /// Requested zero-based sequence index.
        requested: usize,
        /// Number of decoded sequence records.
        available: usize,
    },
    /// Stock disabled an external or aliased sequence whose payload is absent.
    #[error("M2 animation sequence {sequence} has no available payload")]
    SequenceUnavailable {
        /// Requested zero-based sequence index.
        sequence: usize,
    },
    /// NaN or infinity cannot participate in deterministic animation clocks.
    #[error("M2 animation clock contains a non-finite time")]
    NonFiniteTime,
    /// Billboard composition requires the active camera/view basis.
    #[error("M2 billboard bone {bone} requires a camera-relative pose path")]
    BillboardViewRequired {
        /// Zero-based bone index carrying one of stock's billboard flags.
        bone: usize,
    },
    /// The model-to-view transform cannot be inverted for billboard recovery.
    #[error("M2 billboard pose requires a finite, invertible model-view transform")]
    InvalidModelView,
}
