//! Observable facts for one queued UI frame.

/// Draw count accepted by one stock UI presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiFrameReport {
    draw_count: usize,
}

impl UiFrameReport {
    pub(super) const fn new(draw_count: usize) -> Self {
        Self { draw_count }
    }

    /// Returns the number of indexed material batches recorded.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        self.draw_count
    }
}
