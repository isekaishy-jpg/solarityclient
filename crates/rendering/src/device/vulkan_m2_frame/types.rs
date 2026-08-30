//! Observable facts for one queued M2 frame.

/// Draw and transform counts accepted by one M2 presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2FrameReport {
    draw_count: usize,
    bone_transform_count: usize,
}

impl M2FrameReport {
    /// Captures the submitted scene payload after validation succeeds.
    pub(super) const fn new(draw_count: usize, bone_transform_count: usize) -> Self {
        Self {
            draw_count,
            bone_transform_count,
        }
    }

    /// Returns the number of indexed material draws recorded.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        self.draw_count
    }

    /// Returns the number of model transforms uploaded to the bone buffer.
    #[must_use]
    pub const fn bone_transform_count(self) -> usize {
        self.bone_transform_count
    }
}
