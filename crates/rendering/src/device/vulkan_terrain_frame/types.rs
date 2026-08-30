//! Observable facts for one queued terrain frame.

/// Draw count accepted by one terrain presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainFrameReport {
    draw_count: usize,
}

impl TerrainFrameReport {
    pub(super) const fn new(draw_count: usize) -> Self {
        Self { draw_count }
    }

    /// Returns the number of camera-selected MCNK ranges recorded.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        self.draw_count
    }
}
