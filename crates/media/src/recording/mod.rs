//! Explicit 30 FPS hardware video recording with bounded audio and frame queues.

mod audio;
mod encoder;
mod files;
mod session;

#[cfg(test)]
#[path = "../../tests/recording/mod.rs"]
mod tests;

pub use audio::RecordingAudio;
pub use files::RecordingMode;
pub use session::{RecordingReport, VideoRecorder};

/// A recording failure is reported independently of normal gameplay.
#[derive(Debug, thiserror::Error)]
#[error("video recording failed: {0}")]
pub struct RecordingError(String);

impl RecordingError {
    pub(crate) fn new(error: impl std::fmt::Display) -> Self {
        Self(error.to_string())
    }
}

impl From<ffmpeg_next::Error> for RecordingError {
    fn from(error: ffmpeg_next::Error) -> Self {
        Self::new(error)
    }
}

impl From<std::io::Error> for RecordingError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error)
    }
}
