//! Stable identities for retained cinematic pixels.

/// Identifies one authored decoded frame across display refreshes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CinematicFrameIdentity {
    generation: u64,
    frame_index: u64,
}

impl CinematicFrameIdentity {
    /// Joins a Glue movie generation to its zero-based decoded frame index.
    #[must_use]
    pub const fn new(generation: u64, frame_index: u64) -> Self {
        Self {
            generation,
            frame_index,
        }
    }
}
