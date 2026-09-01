//! Dependency-neutral decoded cinematic frame vocabulary.

use std::time::Duration;

/// One tightly packed RGBA8 frame at its authored presentation time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CinematicVideoFrame {
    width: u32,
    height: u32,
    presentation_time: Duration,
    rgba8: Vec<u8>,
}

impl CinematicVideoFrame {
    pub(super) fn new(
        width: u32,
        height: u32,
        presentation_time: Duration,
        rgba8: Vec<u8>,
    ) -> Result<Self, ()> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(rgba8.len()) {
            return Err(());
        }
        Ok(Self {
            width,
            height,
            presentation_time,
            rgba8,
        })
    }

    /// Returns the visible frame width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the visible frame height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the authored presentation time relative to movie start.
    #[must_use]
    pub const fn presentation_time(&self) -> Duration {
        self.presentation_time
    }

    /// Returns tightly packed top-to-bottom RGBA8 pixels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }
}
