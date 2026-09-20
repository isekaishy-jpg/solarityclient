//! Completed job palettes remain borrowed until the renderer finishes its upload.

use super::GeometryBatch;
use glam::Mat4;
use solarity_rendering::M2BonePaletteSource;

impl GeometryBatch {
    /// Logical bone count follows ordered publication, including shadow-only work.
    pub(in super::super::super) fn bone_count(&self) -> usize {
        self.published_bones
    }
}

impl M2BonePaletteSource for GeometryBatch {
    fn len(&self) -> usize {
        self.published_bones
    }
    fn palette_count(&self) -> usize {
        debug_assert!(
            !self.submitted,
            "palette borrow requires reclaimed geometry"
        );
        self.jobs.len()
    }
    fn palette(&self, index: usize) -> &[Mat4] {
        let job = &self.jobs[index];
        if job.publishes_palette {
            job.pose.transforms()
        } else {
            &[]
        }
    }
}
