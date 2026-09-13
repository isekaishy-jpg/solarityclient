//! Observable facts for one queued M2 frame.

/// Draw and transform counts accepted by one M2 presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2FrameReport {
    draw_count: usize,
    submission_count: usize,
    bone_transform_count: usize,
}

impl M2FrameReport {
    /// Captures the submitted scene payload after validation succeeds.
    pub(super) const fn new(
        draw_count: usize,
        bone_transform_count: usize,
        submission_count: usize,
    ) -> Self {
        Self {
            draw_count,
            submission_count,
            bone_transform_count,
        }
    }

    /// Returns the number of logical material instances recorded.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        self.draw_count
    }

    /// Returns indexed Vulkan submissions after compatible instancing.
    pub const fn submission_count(self) -> usize {
        self.submission_count
    }

    /// Returns the number of model transforms uploaded to the bone buffer.
    #[must_use]
    pub const fn bone_transform_count(self) -> usize {
        self.bone_transform_count
    }
}
