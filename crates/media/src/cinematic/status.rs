//! Stable failures for movie demux, decode, timing, and pixel conversion.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// A stock AVI could not produce an exact timestamped video frame stream.
#[derive(Debug, Error)]
pub enum CinematicError {
    /// FFmpeg process initialization failed.
    #[error("failed to initialize cinematic decoder: {message}")]
    Initialization {
        /// Dependency context.
        message: String,
    },
    /// The container exposed no video stream.
    #[error("client cinematic {path} has no video stream")]
    MissingVideo {
        /// Concrete locale-loose AVI.
        path: PathBuf,
    },
    /// The selected video stream ended without yielding a frame.
    #[error("client cinematic {path} has no decoded video frames")]
    EmptyVideo {
        /// Concrete locale-loose AVI.
        path: PathBuf,
    },
    /// A decoded video frame omitted the authored presentation timestamp.
    #[error("client cinematic {path} produced a video frame without a timestamp")]
    MissingTimestamp {
        /// Concrete locale-loose AVI.
        path: PathBuf,
    },
    /// A decoded timestamp cannot form a monotonic process duration.
    #[error("client cinematic {path} produced invalid video timestamp {timestamp}")]
    InvalidTimestamp {
        /// Concrete locale-loose AVI.
        path: PathBuf,
        /// Unscaled FFmpeg presentation timestamp.
        timestamp: i64,
    },
    /// Decoded dimensions overflowed the tightly packed RGBA representation.
    #[error("client cinematic {path} has invalid decoded frame extent {width}x{height}")]
    FrameSize {
        /// Concrete locale-loose AVI.
        path: PathBuf,
        /// Decoded width.
        width: u32,
        /// Decoded height.
        height: u32,
    },
    /// FFmpeg returned a plane row smaller than its visible RGBA pixels.
    #[error("client cinematic {path} RGBA stride {stride} is smaller than visible row {row_bytes}")]
    FrameStride {
        /// Concrete locale-loose AVI.
        path: PathBuf,
        /// Dependency-provided bytes per row.
        stride: usize,
        /// Visible tightly packed bytes per row.
        row_bytes: usize,
    },
    /// An FFmpeg operation failed.
    #[error("failed to {operation} for client cinematic {path}: {message}")]
    Adapter {
        /// Concrete locale-loose AVI.
        path: PathBuf,
        /// Stable operation label.
        operation: &'static str,
        /// Dependency context.
        message: String,
    },
}

impl CinematicError {
    pub(super) fn adapter(
        path: &Path,
        operation: &'static str,
        source: impl std::fmt::Display,
    ) -> Self {
        Self::Adapter {
            path: path.to_path_buf(),
            operation,
            message: source.to_string(),
        }
    }
}
